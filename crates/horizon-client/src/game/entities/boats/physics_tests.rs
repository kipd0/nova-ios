//! The driver resolves one movement for the visible hull and its vehicle packet.
use super::*;
use crate::game::Keys;
use horizon_protocol::packets::play::Serverbound;

#[path = "../../../../../horizon-mod-wasm/tests/fixture/src/boat.rs"]
mod listener;

struct Native;

#[mod_api::__subscriptions]
impl mod_api::Hooks for Native {
    fn controlled_boat_physics(
        &self,
        _: &dyn mod_api::ClientHost,
        event: &mut mod_api::ControlledBoatPhysicsEvent<'_>,
    ) {
        listener::physics(event);
    }
}

fn course(mods: mod_api::Mods, edited: bool) {
    let mut ledge_packet = None;
    mod_platform::testing::with_mods(mods, || {
        crate::game::native_entity_refs::tests::with_client_game(|game| {
            let (client, _server) = horizon_protocol::connection::Pipe::pair();
            game.connect_local(client, "boat-driver").unwrap();
            let remote = game.remote.as_mut().unwrap();
            remote.joined = true;
            remote.first_passenger = true;
            game.streaming
                .insert_remote(&mut game.world, horizon_client::chunk::Chunk::empty(0, 0));
            let stone = game
                .pack
                .assets
                .registry
                .default_state("minecraft:stone")
                .unwrap();
            for x in 4..8 {
                for z in 4..9 {
                    game.world.set(x, 99, z, stone).unwrap();
                    if z >= 6 {
                        game.world.set(x, 100, z, stone).unwrap();
                    }
                }
            }
            for id in [30441, 30442, 30443] {
                game.boats.clear();
                game.riding = Some(id);
                let jump = id == 30442;
                let y = if jump { 105.0 } else { 100.0 };
                let mut boat = Boat::new(id, "oak".into(), Vec3::new(5.5, y, 5.2), 0.0);
                boat.velocity.z = 0.35;
                boat.input = [false, true, true, false];
                boat.on_ground = !jump;
                game.boats.push(boat);
                game.remote_ride(
                    Keys {
                        jump,
                        ..Default::default()
                    },
                    0,
                )
                .unwrap();
                let hull = &game.boats[0];
                if edited && id == 30441 {
                    assert_eq!(hull.position.y, 101.0);
                    assert!(hull.position.z > 5.6, "accelerates over the step");
                    assert_eq!(hull.yaw, 3.0);
                } else if jump {
                    let expected = if edited { 105.49 } else { 104.96 };
                    assert!((hull.position.y - expected).abs() < 1e-7);
                } else {
                    // Unmodified boat or invalid listener output: the block face stops the hull.
                    assert_eq!(hull.position.y, 100.0);
                    assert_eq!(hull.position.z, 5.3125);
                    assert_eq!(hull.yaw, 1.0);
                    assert!(hull.position.x.is_finite());
                }
                let packets = game.remote_vehicle_packets();
                let Serverbound::MoveVehicle(packet) = &packets[0] else {
                    panic!("missing vehicle movement")
                };
                assert_eq!(
                    packet.position,
                    [hull.position.x, hull.position.y, hull.position.z]
                );
                assert_eq!(packet.yaw, hull.yaw);
                assert_eq!(packet.on_ground, hull.on_ground);
                if edited && id == 30441 {
                    ledge_packet = Some(packet.clone());
                }
            }
        });
    });
    if let Some(packet) = ledge_packet {
        crate::game::native_entity_move_correction_tests::accept_ledge_packet(packet);
    }
}

#[test]
#[ignore = "daily"]
fn vanilla_and_native_controlled_boat_course() {
    course(Vec::new().into(), false);
    course(
        vec![mod_api::ModEntry::new("boat_test", Box::new(Native))].into(),
        true,
    );
}

#[test]
#[ignore = "needs HORIZON_TEST_RESOURCES, HORIZON_WASM_FIXTURE and HORIZON_WASM_STD"]
fn compiled_wasm_controlled_boat_course() {
    horizon_guest_stack::run_on_guest_stack(|| {
        use horizon_mod_package::manifest::Package;
        let path = std::path::PathBuf::from(std::env::var_os("HORIZON_WASM_FIXTURE").unwrap());
        let manifest = Package::read(&path).unwrap().manifest;
        let info = mod_api::PackageInfo::new(
            manifest.id.clone(),
            manifest.version.clone(),
            mod_api::PackageKind::Code,
            mod_api::Side::Both,
            vec![manifest.id.clone()],
            vec![],
        );
        let loaded = horizon_mod_wasm::load(
            &std::fs::read(std::env::var_os("HORIZON_WASM_STD").unwrap()).unwrap(),
            vec![horizon_mod_wasm::WasmPackage {
                manifest,
                module: Package::read_wasm(&path).unwrap(),
            }],
        );
        assert!(loaded.refused.is_empty(), "{:?}", loaded.refused);
        assert_eq!(loaded.mods.len(), 1);
        loaded.initialize(&[info]).unwrap();
        course(loaded.mods.into(), true);
    });
}
