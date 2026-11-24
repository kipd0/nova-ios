//! The peer waits for the client's raw opening message before sending any configuration
//! packet. A client waiting for a Horizon offer or FinishConfiguration cannot pass.
use super::*;
use crate::raw_payloads::{Endpoint, Side, Transport};
use mod_api::{Hooks, ModEntry, PayloadBytes, PayloadId, PayloadSendError, Registrar};
use std::net::TcpListener;

const CHANNEL: &str = "example:configuration";

struct Native;
#[mod_api::__subscriptions]
impl Hooks for Native {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        r.payload_handlers().registrar("1").configuration_raw(
            PayloadId(CHANNEL.into()),
            Some(PayloadBytes(vec![0, 1, 128, 255])),
        );
    }
}

#[test]
fn native_configuration_raw_precedes_play_without_offer() {
    exercise(
        vec![ModEntry::new("native_probe", Box::new(Native))],
        "native",
    );
}

#[test]
#[ignore = "requires compiled HORIZON_WASM_FIXTURE and HORIZON_WASM_STD"]
fn wasm_configuration_raw_precedes_play_without_offer() {
    horizon_guest_stack::run_on_guest_stack(|| {
        use horizon_mod_package::manifest::Package;
        let path = PathBuf::from(std::env::var_os("HORIZON_WASM_FIXTURE").unwrap());
        let manifest = Package::read(&path).unwrap().manifest;
        let info = |id: String| {
            mod_api::PackageInfo::new(
                id.clone(),
                "0.1.0".into(),
                mod_api::PackageKind::Code,
                mod_api::Side::Both,
                vec![id],
                vec![],
            )
        };
        let inventory = [
            info(manifest.id.clone()),
            info("raw_configuration_probe".into()),
        ];
        let loaded = horizon_mod_wasm::load(
            &std::fs::read(std::env::var_os("HORIZON_WASM_STD").unwrap()).unwrap(),
            vec![horizon_mod_wasm::WasmPackage {
                manifest,
                module: Package::read_wasm(&path).unwrap(),
            }],
        );
        assert!(loaded.refused.is_empty(), "{:?}", loaded.refused);
        loaded.initialize(&inventory).unwrap();
        exercise(loaded.mods, "wasm");
    });
}

fn exercise(mods: Vec<ModEntry>, case: &str) {
    mod_platform::testing::with_mods(mods.into(), || {
        mod_platform::__private::part_tests::install_parts_fixture(mod_api::__private::mods())
            .unwrap();
        let transport = Transport::capture(Side::Client).unwrap();
        assert!(!transport.local.has_mod_declarations());
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = Connection::connect(
            "127.0.0.1",
            listener.local_addr().unwrap().port(),
            Duration::from_secs(10),
        )
        .unwrap();
        let mut server = Connection::from_stream(listener.accept().unwrap().0).unwrap();
        let (mut reader, writer) = client.split();
        let timeout = Some(Duration::from_secs(10));
        reader.set_read_timeout(timeout).unwrap();
        server.reader.set_read_timeout(timeout).unwrap();
        let (root, data) = tests::empty_item_data(&format!("raw-{case}"));
        let writer = Arc::new(Mutex::new(Some(QueuedWriter::new(writer).unwrap())));
        let configuring = Arc::new(AtomicBool::new(true));
        let (events, _receiver) = event_channel();
        let endpoint = Endpoint::client(
            writer.clone(),
            configuring.clone(),
            transport.clone(),
            events.clone(),
        );
        let id = PayloadId(CHANNEL.into());
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                configure(
                    &mut reader,
                    &writer,
                    &configuring,
                    &Language::new(),
                    &events,
                    &Cookies::default(),
                    &data,
                    &transport,
                    true,
                )
            });
            let receive = |server: &mut Connection| {
                let (packet, body) = server.recv().unwrap();
                match cfg::Serverbound::decode(packet, &body).unwrap() {
                    cfg::Serverbound::CustomPayload(p) => {
                        assert_eq!(p.channel, CHANNEL);
                        p.data
                    }
                    p => panic!("expected a raw configuration message, received {p:?}"),
                }
            };
            assert_eq!(receive(&mut server), [0, 1, 128, 255]);
            let epoch = transport.epoch().unwrap();
            assert_eq!(
                endpoint
                    .prepare(&epoch, &id, PayloadBytes(vec![0; 32_768]))
                    .err(),
                Some(PayloadSendError::PayloadTooLarge {
                    id: id.clone(),
                    size: 32_768,
                    limit: 32_767
                })
            );
            let body = vec![7; 32_767];
            let send = endpoint
                .prepare(&epoch, &id, PayloadBytes(body.clone()))
                .unwrap();
            epoch.with_active(|| send.publish()).unwrap().unwrap();
            assert_eq!(receive(&mut server), body);
            server
                .send(&cfg::Clientbound::FinishConfiguration(Empty).encode())
                .unwrap();
            let (packet, body) = server.recv().unwrap();
            assert!(matches!(
                cfg::Serverbound::decode(packet, &body).unwrap(),
                cfg::Serverbound::FinishConfiguration(_)
            ));
            worker
                .join()
                .unwrap()
                .unwrap_or_else(|reason| panic!("{}", reason.text));
            assert_eq!(
                endpoint.prepare(&epoch, &id, PayloadBytes(vec![])).err(),
                Some(PayloadSendError::StaleEpoch)
            );
            let play = transport.epoch().unwrap();
            assert_eq!(
                endpoint.prepare(&play, &id, PayloadBytes(vec![])).err(),
                Some(PayloadSendError::WrongProtocol(id.clone()))
            );
            transport.close();
            assert_eq!(
                endpoint.prepare(&play, &id, PayloadBytes(vec![])).err(),
                Some(PayloadSendError::StaleEpoch)
            );
        });
        std::fs::remove_dir_all(root).unwrap();
    });
}
