use s2protocol_port::ProtocolStoreBuilder;

#[test]
fn protocol_store_includes_latest_upstream_builds() {
    let store = ProtocolStoreBuilder::build().expect("protocol store should build");
    let builds = store.known_builds();

    assert_eq!(builds.len(), 91);
    assert!(builds.contains(&97364));
    assert!(builds.contains(&97425));
    assert_eq!(builds.last().copied(), Some(97425));
    assert_eq!(
        store
            .latest()
            .expect("latest protocol should exist")
            .build(),
        97425
    );
}
