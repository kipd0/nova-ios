# Nova iOS — Microsoft accounts

This feature is confined to `platforms/ios`; the upstream Horizon
game/online-mode implementation stays unchanged. The native login and
credential-storage logic is adapted from Kelly-Inc/horizon-ios (MIT notice in
`KELLY_AUTH_LICENSE.txt`).

## Distributor setup

Microsoft sign-in requires a **public-client application ID** that can use
consumer Microsoft accounts and the device-code OAuth flow. A registration
must allow public client flows and must be approved for Minecraft Services.
Creating a Microsoft app registration alone does **not** authorize Minecraft
Java authentication.

Set the GitHub Actions **repository variable** `HORIZON_MSA_CLIENT_ID`
(Settings → Secrets and variables → Actions → Variables) to the approved
application client GUID. No Microsoft client secret should be created,
included in the IPA or stored in GitHub.

The iOS build passes the public ID as `NOVA_MSA_CLIENT_ID`, and XcodeGen
writes it to Info.plist as `HorizonMSAClientID`. An empty or invalid ID
disables sign-in and displays a clear message. This port does not borrow an
unrelated application's ID.

## User flow

Tap the upper-left Minecraft head/name, then Add Microsoft account. Nova
requests a one-time code from Microsoft's consumer OAuth endpoint. The player
opens https://www.microsoft.com/link in the browser and signs in directly to
Microsoft — the password never touches Nova.

Nova exchanges the grant through Xbox Live, XSTS and Minecraft Services,
checks Java Edition entitlement, fetches the Minecraft UUID and username,
and commits refresh/access tokens only to the app's Keychain. Additional
accounts can be saved; choose one in the same sheet. Names and UUIDs alone
are kept in app preferences; the head uses mc-heads by Minecraft UUID.
The account selector shows an explicit restart notice.

On the **next cold start**, the selected session is checked and refreshed
if near expiry. Nova writes a one-use JSON handoff with file mode 0600 in
the app sandbox's **private temporary directory**, not Documents or the
Files-visible Nova folders; the existing Horizon engine reads it via
`HORIZON_SESSION_FILE` and deletes it immediately. The game sets its
player identity from the authenticated session and uses Minecraft's
online-mode join handshake. Failed renewal/Keychain access reports a
generic message and does not invent an online session.

## Diagnostics and privacy

- The account code never receives the Microsoft password.
- Keychain records use `WhenUnlockedThisDeviceOnly`; they are not exported
  with saves, worlds, resource packs or backups.
- Account HTTP calls use ephemeral URL sessions. Sensitive replies,
  headers, device codes, refresh tokens and access tokens are never
  passed to Nova diagnostics or printed.
- Unfiltered stdout/stderr redirection into the user-exportable
  `Diagnostics` folder is disabled. Nova's structured diagnostic
  records also redact common token and email patterns.
- The game has separate Horizon logs; inspect logs before sharing.
  These precautions reduce exposure, but are not a substitute for
  security review and testing.

## Required physical-device checks before release

1. Configure the approved app ID, build an IPA and sign in on an iPad.
2. Verify that the *actual* Java username and mc-heads skin appear.
3. Restart the app fully; connect to an online-mode Minecraft server.
4. Wait until tokens need renewal, relaunch with and without network,
   and confirm the correct account remains selected.
5. Add a second Microsoft account, switch, restart and verify that the
   server sees the second UUID; switch offline and restart again.
6. Test cancellations, expired device codes, denied entitlement, locked
   Keychain and sign-out/remove.
7. Inspect Files → Nova → Diagnostics and Game/logs for accidental
   sensitive data. Verify no session JSON remains in the private
   temporary directory after Horizon has consumed it.

This change has not been certified on a physical device. Being able to join
a particular server also depends on account eligibility, service
availability and server configuration.
