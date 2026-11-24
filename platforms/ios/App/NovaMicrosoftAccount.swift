// Derived from Kelly-Inc/horizon-ios packaging/ios/Launcher code.
// Copyright (c) 2026 Ludovic Marie and the Horizon contributors.
// Licensed under MIT; see platforms/ios/KELLY_AUTH_LICENSE.txt.

import Security
import SwiftUI
import UIKit

@MainActor
final class NovaMicrosoftAccount: ObservableObject {
    struct Profile: Codable, Equatable, Identifiable {
        let name: String
        let uuid: String
        var id: String { uuid }
    }
    struct Catalog: Codable { var profiles: [Profile]; var selected: String? }
    enum State: Equatable {
        case idle, code(String, URL), working(String), failed(String)
    }
    @Published private(set) var profile: Profile?
    @Published private(set) var accounts: [Profile] = []
    @Published private(set) var state = State.idle
    private let defaults: UserDefaults
    private let vault: AccountVault
    private let net: AccountNet
    private let now: () -> Date
    private let sleep: (UInt64) async throws -> Void
    private let client: String?
    private var signing: Task<Void, Never>?
    private var generation = UUID()
    private var selection = UUID()
    private var renewals: [String: Task<(Profile, AccountSecrets), Error>] = [:]
    private var migrationPending = false
    private var legacyProfile: Profile?
    private var invalidSelection = false
    var needsAccountSelection: Bool { invalidSelection }
    nonisolated static var clientID: String? {
        (Bundle.main.object(forInfoDictionaryKey: "HorizonMSAClientID") as? String)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .flatMap { $0.isEmpty || $0.hasPrefix("$(") || UUID(uuidString: $0) == nil ? nil : $0 }
    }
    init(defaults: UserDefaults = .standard, vault: AccountVault = KeychainAccountVault(),
         session: URLSession = URLSession(configuration: .ephemeral), clock: @escaping () -> Date = Date.init,
         sleep: @escaping (UInt64) async throws -> Void = { try await Task.sleep(nanoseconds: $0) },
         client: String? = NovaMicrosoftAccount.clientID) {
        self.defaults = defaults; self.vault = vault; net = AccountNet(session: session)
        now = clock; self.sleep = sleep; self.client = client
        if let data = defaults.data(forKey: "microsoftAccounts.v2"),
           let saved = try? JSONDecoder().decode(Catalog.self, from: data) {
            accounts = saved.profiles
            profile = accounts.first { $0.uuid == saved.selected }
            invalidSelection = saved.selected != nil && profile == nil
        } else if let data = defaults.data(forKey: "microsoftProfile"),
                  let old = try? JSONDecoder().decode(Profile.self, from: data) {
            accounts = [old]; profile = old
        }
        if let data = defaults.data(forKey: "microsoftProfile"),
           let old = try? JSONDecoder().decode(Profile.self, from: data) {
            legacyProfile = old; migrationPending = true
            do { try migrate() } catch { state = .failed((error as? AccountError)?.localizedDescription ?? "Account operation failed. Try again.") }
        }
    }
    private func saveCatalog() throws {
        defaults.set(try JSONEncoder().encode(Catalog(profiles: accounts, selected: profile?.uuid)),
                     forKey: "microsoftAccounts.v2")
    }
    private func migrate() throws {
        guard migrationPending, let old = legacyProfile else { return }
        // Copy successfully before removing either legacy record. A locked
        // Keychain leaves migration retryable and the original credentials intact.
        if try vault.read(old.uuid) == nil {
            guard let secrets = try vault.read("account") else {
                throw AccountError("Sign in again to restore this account.", reauthenticate: true)
            }
            try vault.write(secrets, for: old.uuid)
        }
        try saveCatalog()
        try vault.remove("account")
        defaults.removeObject(forKey: "microsoftProfile")
        migrationPending = false; legacyProfile = nil
    }
    func cancel() {
        generation = UUID(); signing?.cancel(); signing = nil; state = .idle
    }
    func select(_ selected: Profile?) {
        cancel(); selection = UUID(); profile = selected; invalidSelection = false
        do { try saveCatalog() } catch { state = .failed((error as? AccountError)?.localizedDescription ?? "Account operation failed. Try again.") }
    }
    func remove(_ account: Profile) async {
        cancel()
        renewals.removeValue(forKey: account.uuid)?.cancel()
        do {
            try vault.remove(account.uuid)
            if legacyProfile?.uuid == account.uuid {
                try vault.remove("account")
                defaults.removeObject(forKey: "microsoftProfile")
                legacyProfile = nil; migrationPending = false
            }
            accounts.removeAll { $0.uuid == account.uuid }
            if profile?.uuid == account.uuid { selection = UUID(); profile = nil }
            try saveCatalog()
        } catch { state = .failed((error as? AccountError)?.localizedDescription ?? "Account operation failed. Try again.") }
    }
    func signOut() { select(nil) }

    private func credentials(for account: Profile) async throws -> (Profile, AccountSecrets) {
        if let task = renewals[account.uuid] { return try await task.value }
        guard let secrets = try vault.read(account.uuid) else {
            throw AccountError("Sign in again to restore \(account.name).", reauthenticate: true)
        }
        if !secrets.token.isEmpty, secrets.expires.timeIntervalSince(now()) > 300 { return (account, secrets) }
        guard let client else { throw AccountError("Microsoft sign-in isn't configured in this build.") }
        let task = Task { @MainActor in
            let microsoft = try await Self.refresh(client, secrets.refresh)
            try Task.checkCancellation()
            // Microsoft rotates this credential before Xbox/Minecraft requests.
            // Persist now so a downstream outage cannot discard the new token.
            try self.vault.write(AccountSecrets(refresh: microsoft.refresh, token: secrets.token,
                                                 expires: secrets.expires), for: account.uuid)
            let result = try await Self.exchange(microsoft: microsoft)
            try Task.checkCancellation()
            guard result.0.uuid == account.uuid else { throw AccountError("Microsoft returned a different account. Sign in again.", reauthenticate: true) }
            try self.vault.write(result.1, for: account.uuid)
            if let index = self.accounts.firstIndex(where: { $0.uuid == account.uuid }) {
                self.accounts[index] = result.0
                if self.profile?.uuid == account.uuid { self.profile = result.0 }
                try self.saveCatalog()
            }
            return result
        }
        renewals[account.uuid] = task
        defer { renewals[account.uuid] = nil }
        return try await task.value
    }
    func signIn() {
        guard let client else { state = .failed("Microsoft sign-in isn't configured in this build."); return }
        cancel()
        let attempt = generation
        let epoch = selection
        signing = Task {
            defer { if generation == attempt { signing = nil } }
            do {
                try Task.checkCancellation()
                let tokens = try await deviceCode(client)
                try Task.checkCancellation()
                guard generation == attempt, selection == epoch else { throw CancellationError() }
                state = .working("Signing in to Minecraft…")
                let (added, secrets) = try await Self.exchange(microsoft: tokens)
                try Task.checkCancellation()
                guard generation == attempt, selection == epoch else { throw CancellationError() }
                if let renewal = renewals[added.uuid] { _ = try? await renewal.value }
                try Task.checkCancellation()
                guard generation == attempt, selection == epoch else { throw CancellationError() }
                try vault.write(secrets, for: added.uuid)
                if let i = accounts.firstIndex(where: { $0.uuid == added.uuid }) { accounts[i] = added }
                else { accounts.append(added) }
                try saveCatalog()
                // Select only after ownership/profile validation and Keychain storage.
                // The newly selected identity takes effect after a full restart.
                profile = added; invalidSelection = false; selection = UUID(); try saveCatalog()
                state = .idle; signing = nil
            } catch is CancellationError {
                if generation == attempt { state = .idle }
            } catch {
                guard generation == attempt else { return }
                state = .failed((error as? AccountError)?.localizedDescription ?? "Sign-in failed. Try again.")
            }
        }
    }
    /// Steps 1 and 2: a code for the player to enter, then the wait for them to do it.
    private func deviceCode(_ client: String) async throws -> (access: String, refresh: String) {
        try Task.checkCancellation()
        state = .working("Asking Microsoft for a code…")
        let start = try await net.form("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode",
                                       ["client_id": client, "scope": "XboxLive.signin offline_access"])
        try Task.checkCancellation()
        guard let device = start["device_code"] as? String, let code = start["user_code"] as? String,
              let link = (start["verification_uri"] as? String).flatMap(URL.init(string:)),
              link.scheme == "https"
        else { throw AccountError("Microsoft didn't give a sign-in code.") }
        state = .code(code, link)
        var interval = UInt64(min(60, max(1, start["interval"] as? Int ?? 5)))
        let deadline = now().addingTimeInterval(TimeInterval(start["expires_in"] as? Int ?? 900))
        while now() < deadline {
            try await sleep(interval * 1_000_000_000)
            try Task.checkCancellation()
            let answer = try await net.form(
                "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
                ["grant_type": "urn:ietf:params:oauth:grant-type:device_code", "client_id": client,
                 "device_code": device],
                allowErrors: true
            )
            switch answer["error"] as? String {
            case nil:
                guard let access = answer["access_token"] as? String,
                      let refresh = answer["refresh_token"] as? String
                else { throw AccountError("Microsoft's answer had no token.") }
                return (access, refresh)
            case "authorization_pending": continue
            case "slow_down": interval = min(60, interval + 5)
            case "authorization_declined": throw AccountError("The sign-in was declined.")
            case "expired_token": throw AccountError("The code expired. Try again.")
            default: throw AccountError("Microsoft refused the sign-in. Try again or check this build’s app configuration.")
            }
        }
        throw AccountError("The code expired. Try again.")
    }

    nonisolated static func refresh(_ client: String, _ token: String) async throws -> (access: String, refresh: String) {
        let net = AccountNet(session: URLSession(configuration: .ephemeral))
        let answer = try await net.form(
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
            ["grant_type": "refresh_token", "client_id": client, "refresh_token": token,
             "scope": "XboxLive.signin offline_access"],
            allowErrors: true
        )
        if let error = answer["error"] as? String {
            if ["invalid_grant", "interaction_required", "consent_required"].contains(error) {
                throw AccountError("Microsoft requires you to sign in again. Your saved account has been kept.", reauthenticate: true)
            }
            throw AccountError("Microsoft could not renew the session. Check your connection and try again.")
        }
        guard answer["status"] == nil,
              let access = answer["access_token"] as? String, !access.isEmpty else {
            throw AccountError("Microsoft returned an incomplete session. Try again.")
        }
        return (access, answer["refresh_token"] as? String ?? token)
    }

    /// Steps 3 to 7: Xbox Live, XSTS, Minecraft, ownership and the profile; then keep it all.
    nonisolated static func exchange(microsoft: (access: String, refresh: String)) async throws -> (Profile, AccountSecrets) {
        let net = AccountNet(session: URLSession(configuration: .ephemeral))
        let xbl = try await net.json("https://user.auth.xboxlive.com/user/authenticate", [
            "Properties": ["AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com",
                           "RpsTicket": "d=\(microsoft.access)"],
            "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT",
        ])
        guard let xblToken = xbl["Token"] as? String,
              let uhs = (((xbl["DisplayClaims"] as? [String: Any])?["xui"] as? [[String: Any]])?.first)?["uhs"] as? String
        else { throw AccountError("Xbox Live didn't sign you in.") }
        let xsts = try await net.json("https://xsts.auth.xboxlive.com/xsts/authorize", [
            "Properties": ["SandboxId": "RETAIL", "UserTokens": [xblToken]] as [String: Any],
            "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT",
        ], allowErrors: true)
        if let code = xsts["XErr"] as? Int {
            throw AccountError(Self.xboxError(code))
        }
        guard xsts["status"] == nil, let xstsToken = xsts["Token"] as? String else { throw AccountError("Xbox Live refused the sign-in.") }
        let minecraft = try await net.json("https://api.minecraftservices.com/authentication/login_with_xbox",
                                           ["identityToken": "XBL3.0 x=\(uhs);\(xstsToken)"], allowErrors: true)
        guard minecraft["status"] == nil, let token = minecraft["access_token"] as? String, !token.isEmpty else {
            if minecraft["status"] as? Int == 403 || (minecraft["errorMessage"] as? String)?.contains("Invalid app") == true {
                throw AccountError("This app is not approved by Mojang yet.")
            }
            throw AccountError("Minecraft refused the sign-in.")
        }
        guard let lifetime = minecraft["expires_in"] as? Double, lifetime.isFinite, lifetime > 0 else {
            throw AccountError("Minecraft did not provide a valid session expiry. Try again.")
        }
        let expires = Date().addingTimeInterval(lifetime)
        let store = try await net.get("https://api.minecraftservices.com/entitlements/mcstore", token: token)
        let owned = (store["items"] as? [[String: Any]] ?? []).contains {
            ["game_minecraft", "product_minecraft"].contains($0["name"] as? String ?? "")
        }
        guard owned else { throw AccountError("This account doesn't own Minecraft: Java Edition.") }
        let profile = try await net.get("https://api.minecraftservices.com/minecraft/profile", token: token,
                                        allowErrors: true)
        guard profile["status"] == nil, let id = profile["id"] as? String, let name = profile["name"] as? String else {
            throw AccountError("Create your Minecraft profile on minecraft.net first.")
        }
        try Task.checkCancellation()
        let normalized = id.replacingOccurrences(of: "-", with: "").lowercased()
        guard normalized.count == 32, normalized.allSatisfy({ $0.isHexDigit }), !name.isEmpty else {
            throw AccountError("Minecraft returned an invalid account profile.")
        }
        return (Profile(name: name, uuid: normalized), AccountSecrets(refresh: microsoft.refresh, token: token, expires: expires))
    }

    nonisolated private static func xboxError(_ code: Int) -> String {
        switch code {
        case 2_148_916_233: return "This Microsoft account has no Xbox profile yet: create one at xbox.com."
        case 2_148_916_235: return "Xbox Live isn't available in your country."
        case 2_148_916_236, 2_148_916_237: return "This account needs adult verification on xbox.com."
        case 2_148_916_238: return "This is a child account: an adult has to add it to a Microsoft family."
        default: return "Xbox Live refused the sign-in (\(code))."
        }
    }
}

