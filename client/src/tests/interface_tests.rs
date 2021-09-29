// Copyright 2020-2021 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

use crate::{actors::SnapshotConfig, line_error, utils::LoadFromPath, Location, RecordHint, Stronghold};

use engine::vault::ClientId;
#[cfg(feature = "p2p")]
use p2p::firewall::Rule;

#[cfg(feature = "p2p")]
use crate::{
    actors::p2p::{messages::SwarmInfo, NetworkConfig},
    tests::fresh,
    ProcResult, Procedure, ResultMessage, SLIP10DeriveInput, StatusMessage,
};

#[actix::test]
async fn test_stronghold() {
    let vault_path = b"path".to_vec();
    let client_path = b"test".to_vec();

    let loc0 = Location::counter::<_, usize>("path", 0);
    let loc1 = Location::counter::<_, usize>("path", 1);
    let loc2 = Location::counter::<_, usize>("path", 2);

    let store_loc = Location::generic("some", "path");

    let key_data = b"abcdefghijklmnopqrstuvwxyz012345".to_vec();

    let mut stronghold = Stronghold::init_stronghold_system(client_path.clone(), vec![])
        .await
        .unwrap();

    // clone it, and check for consistency
    let stronghold2 = stronghold.clone();

    // Write at the first record of the vault using Some(0).  Also creates the new vault.
    assert!(stronghold2
        .write_to_vault(
            loc0.clone(),
            b"test".to_vec(),
            RecordHint::new(b"first hint").expect(line_error!()),
            vec![],
        )
        .await
        .is_ok());

    // read head.
    let (p, _) = stronghold2.read_secret(client_path.clone(), loc0.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("test"));

    // read head from first reference
    let (p, _) = stronghold.read_secret(client_path.clone(), loc0.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("test"));

    // Write on the next record of the vault using None.  This calls InitRecord and creates a new one at index 1.
    assert!(stronghold
        .write_to_vault(
            loc1.clone(),
            b"another test".to_vec(),
            RecordHint::new(b"another hint").expect(line_error!()),
            vec![],
        )
        .await
        .is_ok());

    // read head.
    let (p, _) = stronghold.read_secret(client_path.clone(), loc1.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("another test"));

    assert!(stronghold
        .write_to_vault(
            loc2.clone(),
            b"yet another test".to_vec(),
            RecordHint::new(b"yet another hint").expect(line_error!()),
            vec![],
        )
        .await
        .is_ok());

    // read head.
    let (p, _) = stronghold.read_secret(client_path.clone(), loc2.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("yet another test"));

    // Read the first record of the vault.
    let (p, _) = stronghold.read_secret(client_path.clone(), loc0.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("test"));

    // Read the head record of the vault.
    let (p, _) = stronghold.read_secret(client_path.clone(), loc1).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("another test"));

    let (p, _) = stronghold.read_secret(client_path.clone(), loc2.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("yet another test"));

    let (ids, _) = stronghold.list_hints_and_ids(vault_path.clone()).await;
    println!("{:?}", ids);

    stronghold.delete_data(loc0.clone(), true).await;

    // attempt to read the first record of the vault.
    let (p, _) = stronghold.read_secret(client_path.clone(), loc0.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok(""));

    let (ids, _) = stronghold.list_hints_and_ids(vault_path.clone()).await;
    println!("{:?}", ids);

    stronghold
        .write_to_store(store_loc.clone(), b"test".to_vec(), None)
        .await;

    let (data, _) = stronghold.read_from_store(store_loc.clone()).await;

    assert_eq!(std::str::from_utf8(&data), Ok("test"));

    stronghold.garbage_collect(vault_path).await;

    stronghold
        .write_all_to_snapshot(&key_data, Some("test0".into()), None)
        .await;

    stronghold
        .read_snapshot(client_path.clone(), None, &key_data, Some("test0".into()), None)
        .await;

    // read head after reading snapshot.

    let (p, _) = stronghold.read_secret(client_path.clone(), loc2.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("yet another test"));

    let (p, _e) = stronghold.read_secret(client_path.clone(), loc0).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok(""));

    stronghold.kill_stronghold(client_path.clone(), false).await;

    let (p, _) = stronghold.read_secret(client_path.clone(), loc2).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok(""));

    let (data, _) = stronghold.read_from_store(store_loc.clone()).await;

    assert_eq!(std::str::from_utf8(&data), Ok("test"));

    stronghold.delete_from_store(store_loc.clone()).await;

    let (data, _) = stronghold.read_from_store(store_loc).await;

    assert_eq!(std::str::from_utf8(&data), Ok(""));

    stronghold.kill_stronghold(client_path, true).await;

    // actor tree?
    // stronghold.system.print_tree();
}

#[actix::test]
async fn test_fully_synchronize_snapshot() {
    // __setup

    // A
    let client_path0 = b"client_path0".to_vec();
    let client_path1 = b"client_path1".to_vec();
    let client_path2 = b"client_path2".to_vec();
    let client_path3 = b"client_path3".to_vec();

    // B
    let client_path4 = b"client_path4".to_vec();
    let client_path5 = b"client_path5".to_vec();

    // locations A
    let loc_a0 = Location::Generic {
        record_path: b"loc_a0".to_vec(),
        vault_path: b"vault_a0".to_vec(),
    };
    let loc_a1 = Location::Generic {
        record_path: b"loc_a1".to_vec(),
        vault_path: b"vault_a1".to_vec(),
    };
    let loc_a2 = Location::Generic {
        record_path: b"loc_a2".to_vec(),
        vault_path: b"vault_a2".to_vec(),
    };
    let loc_a3 = Location::Generic {
        record_path: b"loc_a3".to_vec(),
        vault_path: b"vault_a3".to_vec(),
    };

    // locations B
    let loc_b0 = Location::Generic {
        record_path: b"loc_b0".to_vec(),
        vault_path: b"vault_b0".to_vec(),
    };
    let loc_b1 = Location::Generic {
        record_path: b"loc_b1".to_vec(),
        vault_path: b"vault_b1".to_vec(),
    };

    // path A
    let mut tf = std::env::temp_dir();
    tf.push("path_a.snapshot");
    let storage_path_a = tf.to_str().unwrap();

    // path B
    let mut tf = std::env::temp_dir();
    tf.push("path_b.snapshot");
    let storage_path_b = tf.to_str().unwrap();

    // path destination
    let mut tf = std::env::temp_dir();
    tf.push("path_destination.snapshot");
    let storage_path_destination = tf.to_str().unwrap();

    // key for snapshot a
    let key_a = b"aaaBBcDDDDcccbbbBBDDD11223344556".to_vec();

    // key for snapshot b
    let key_b = b"lkjhbhnushfzghfjdkslaksjdnfjs2ks".to_vec();

    // key for destination snapshot
    let key_destination = b"12345678912345678912345678912345".to_vec();

    // __execution
    {
        // A
        let mut stronghold = Stronghold::init_stronghold_system(client_path0.clone(), vec![])
            .await
            .unwrap();

        // write into vault for a
        stronghold
            .write_to_vault(
                loc_a0.clone(),
                b"payload_a0".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        stronghold
            .spawn_stronghold_actor(client_path1.clone(), Vec::new())
            .await;
        stronghold.switch_actor_target(client_path1.clone()).await;
        stronghold
            .write_to_vault(
                loc_a1.clone(),
                b"payload_a1".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        stronghold
            .spawn_stronghold_actor(client_path2.clone(), Vec::new())
            .await;
        stronghold.switch_actor_target(client_path2.clone()).await;
        stronghold
            .write_to_vault(
                loc_a2.clone(),
                b"payload_a2".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        stronghold
            .spawn_stronghold_actor(client_path3.clone(), Vec::new())
            .await;
        stronghold.switch_actor_target(client_path3.clone()).await;
        stronghold
            .write_to_vault(
                loc_a3.clone(),
                b"payload_a3".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        // write local snapshot
        stronghold
            .write_all_to_snapshot(&key_a, None, Some(storage_path_a.into()))
            .await;
    }

    {
        // B

        // write snapshot b
        let mut stronghold = Stronghold::init_stronghold_system(client_path4.clone(), vec![])
            .await
            .unwrap();

        stronghold
            .write_to_vault(
                loc_b0.clone(),
                b"payload_a0".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        stronghold
            .spawn_stronghold_actor(client_path5.clone(), Vec::new())
            .await;
        stronghold.switch_actor_target(client_path5.clone()).await;
        stronghold
            .write_to_vault(
                loc_b1.clone(),
                b"payload_a0".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        stronghold
            .write_all_to_snapshot(&key_b, None, Some(storage_path_b.into()))
            .await;
    }

    // load A, partially synchronize with B, test partial entries from A and B
    let mut stronghold = Stronghold::init_stronghold_system(client_path0.clone(), vec![])
        .await
        .unwrap();

    // reload A
    let all_paths = vec![
        client_path0.clone(),
        client_path1.clone(),
        client_path2.clone(),
        client_path3.clone(),
    ];

    let former = client_path0.clone();

    // keep the reference to the current client id
    // synchronize the other client_path with this snapshot
    for client_path in all_paths {
        stronghold
            .read_snapshot(
                client_path.clone(),
                Some(former.clone()),
                &key_a,
                None,
                Some(storage_path_a.into()),
            )
            .await;
    }

    let mut key_a_static = [0u8; 32];
    key_a_static.clone_from_slice(&key_a);

    let mut key_b_static = [0u8; 32];
    key_b_static.clone_from_slice(&key_b);

    let mut key_destination_static = [0u8; 32];
    key_destination_static.clone_from_slice(&key_destination);

    // partially synchronize with other snapshot
    stronghold
        .synchronize_full(
            ClientId::load_from_path(&client_path0.clone(), &client_path0.clone()).unwrap(),
            SnapshotConfig {
                filename: None,
                path: Some(storage_path_a.into()),
                key: key_a_static,
                generates_output: false,
            },
            SnapshotConfig {
                filename: None,
                path: Some(storage_path_b.into()),
                key: key_b_static,
                generates_output: false,
            },
            SnapshotConfig {
                filename: None,
                path: Some(storage_path_destination.into()),
                key: key_destination_static,
                generates_output: false,
            },
        )
        .await;

    // check for existing locations
    assert!(stronghold.vault_exists(loc_a0).await);
    assert!(stronghold.vault_exists(loc_a1).await);
    assert!(stronghold.vault_exists(loc_a2).await);
    assert!(stronghold.vault_exists(loc_a3).await);
    assert!(stronghold.vault_exists(loc_b0).await);
    assert!(stronghold.vault_exists(loc_b1).await);
}

#[actix::test]
async fn test_actor_isolation() {
    let path_a = b"path-a".to_vec();
    let path_b = b"path-b".to_vec();

    let location_a = Location::generic("vault_path0", "record_path0");
    let location_b = Location::generic("vault_path1", "record_path1");

    let mut stronghold = Stronghold::init_stronghold_system(path_a.clone(), vec![])
        .await
        .unwrap();
    stronghold
        .write_to_vault(
            location_a,
            b"payload_a".to_vec(),
            RecordHint::new("hint").unwrap(),
            Vec::new(),
        )
        .await;

    // spawn second stronghold
    stronghold.spawn_stronghold_actor(path_b.clone(), Vec::new()).await;

    // switch current client actor
    assert!(stronghold.switch_actor_target(path_b.clone()).await.is_ok());

    stronghold
        .write_to_vault(
            location_b.clone(),
            b"payload_b".to_vec(),
            RecordHint::new("hint0").unwrap(),
            Vec::new(),
        )
        .await;

    // switch to first client actor
    assert!(stronghold.switch_actor_target(path_a.clone()).await.is_ok());

    // check if entry from b is inside a
    assert!(!stronghold.vault_exists(location_b).await);
}

#[actix::test]
async fn test_partially_synchronize_snapshot() {
    // __setup

    // A
    let client_path0 = b"client_path0".to_vec();
    let client_path1 = b"client_path1".to_vec();
    let client_path2 = b"client_path2".to_vec();
    let client_path3 = b"client_path3".to_vec();

    // B
    let client_path4 = b"client_path4".to_vec();
    let client_path5 = b"client_path5".to_vec();

    // locations A
    let loc_a0 = Location::Generic {
        record_path: b"loc_a0".to_vec(),
        vault_path: b"vault_a0".to_vec(),
    };
    let loc_a1 = Location::Generic {
        record_path: b"loc_a1".to_vec(),
        vault_path: b"vault_a1".to_vec(),
    };
    let loc_a2 = Location::Generic {
        record_path: b"loc_a2".to_vec(),
        vault_path: b"vault_a2".to_vec(),
    };
    let loc_a3 = Location::Generic {
        record_path: b"loc_a3".to_vec(),
        vault_path: b"vault_a3".to_vec(),
    };

    // locations B
    let loc_b0 = Location::Generic {
        record_path: b"loc_b0".to_vec(),
        vault_path: b"vault_b0".to_vec(),
    };
    let loc_b1 = Location::Generic {
        record_path: b"loc_b1".to_vec(),
        vault_path: b"vault_b1".to_vec(),
    };

    // allowed entries from B
    let allowed = vec![ClientId::load_from_path(&client_path5.clone(), &client_path5.clone()).unwrap()];

    // path A
    let mut tf = std::env::temp_dir();
    tf.push("path_a.snapshot");
    let storage_path_a = tf.to_str().unwrap();

    // path B
    let mut tf = std::env::temp_dir();
    tf.push("path_b.snapshot");
    let storage_path_b = tf.to_str().unwrap();

    // path destination
    let mut tf = std::env::temp_dir();
    tf.push("path_destination.snapshot");
    let storage_path_destination = tf.to_str().unwrap();

    // key for snapshot a
    let key_a = b"aaaBBcDDDDcccbbbBBDDD11223344556".to_vec();

    // key for snapshot b
    let key_b = b"lkjhbhnushfzghfjdkslaksjdnfjs2ks".to_vec();

    // key for destination snapshot
    let key_destination = b"12345678912345678912345678912345".to_vec();

    // __execution
    {
        // A
        let mut stronghold = Stronghold::init_stronghold_system(client_path0.clone(), vec![])
            .await
            .unwrap();

        // write into vault for a
        stronghold
            .write_to_vault(
                loc_a0.clone(),
                b"payload_a0".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        stronghold
            .spawn_stronghold_actor(client_path1.clone(), Vec::new())
            .await;
        stronghold.switch_actor_target(client_path1.clone()).await;
        stronghold
            .write_to_vault(
                loc_a1.clone(),
                b"payload_a1".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        stronghold
            .spawn_stronghold_actor(client_path2.clone(), Vec::new())
            .await;
        stronghold.switch_actor_target(client_path2.clone()).await;
        stronghold
            .write_to_vault(
                loc_a2.clone(),
                b"payload_a2".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        stronghold
            .spawn_stronghold_actor(client_path3.clone(), Vec::new())
            .await;
        stronghold.switch_actor_target(client_path3.clone()).await;
        stronghold
            .write_to_vault(
                loc_a3.clone(),
                b"payload_a3".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        // write local snapshot
        stronghold
            .write_all_to_snapshot(&key_a, None, Some(storage_path_a.into()))
            .await;
    }

    {
        // B

        // write snapshot b
        let mut stronghold = Stronghold::init_stronghold_system(client_path4.clone(), vec![])
            .await
            .unwrap();

        stronghold
            .write_to_vault(
                loc_b0.clone(),
                b"payload_b0".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        stronghold
            .spawn_stronghold_actor(client_path5.clone(), Vec::new())
            .await;
        stronghold.switch_actor_target(client_path5.clone()).await;
        stronghold
            .write_to_vault(
                loc_b1.clone(),
                b"payload_b0".to_vec(),
                RecordHint::new(b"record_hint_a0".to_vec()).unwrap(),
                vec![],
            )
            .await;

        stronghold
            .write_all_to_snapshot(&key_b, None, Some(storage_path_b.into()))
            .await;
    }

    // load A, partially synchronize with B, test partial entries from A and B
    let mut stronghold = Stronghold::init_stronghold_system(client_path0.clone(), vec![])
        .await
        .unwrap();

    // reload A
    let all_paths = vec![
        client_path0.clone(),
        client_path1.clone(),
        client_path2.clone(),
        client_path3.clone(),
    ];

    let former = client_path0.clone();

    // keep the reference to the current client id
    // synchronize the other client_path with this snapshot
    for client_path in all_paths {
        stronghold
            .read_snapshot(
                client_path.clone(),
                Some(former.clone()),
                &key_a,
                None,
                Some(storage_path_a.into()),
            )
            .await;
    }

    let mut key_a_static = [0u8; 32];
    key_a_static.clone_from_slice(&key_a);

    let mut key_b_static = [0u8; 32];
    key_b_static.clone_from_slice(&key_b);

    let mut key_destination_static = [0u8; 32];
    key_destination_static.clone_from_slice(&key_destination);

    // partially synchronize with other snapshot
    stronghold
        .synchronize_partial(
            ClientId::load_from_path(&client_path0.clone(), &client_path0.clone()).unwrap(),
            SnapshotConfig {
                filename: None,
                path: Some(storage_path_a.into()),
                key: key_a_static,
                generates_output: false,
            },
            SnapshotConfig {
                filename: None,
                path: Some(storage_path_b.into()),
                key: key_b_static,
                generates_output: false,
            },
            SnapshotConfig {
                filename: None,
                path: Some(storage_path_destination.into()),
                key: key_destination_static,
                generates_output: false,
            },
            allowed,
        )
        .await;

    assert!(!stronghold.vault_exists(loc_b0).await);
    assert!(stronghold.vault_exists(loc_b1).await);
}

#[actix::test]
async fn run_stronghold_multi_actors() {
    let key_data = b"abcdefghijklmnopqrstuvwxyz012345".to_vec();

    let client_path0 = b"test a".to_vec();
    let client_path1 = b"test b".to_vec();
    let client_path2 = b"test c".to_vec();
    let client_path3 = b"test d".to_vec();

    let loc0 = Location::counter::<_, usize>("path", 0);

    let loc2 = Location::counter::<_, usize>("path", 2);
    let loc3 = Location::counter::<_, usize>("path", 3);
    let loc4 = Location::counter::<_, usize>("path", 4);

    let mut stronghold = Stronghold::init_stronghold_system(client_path0.clone(), vec![])
        .await
        .unwrap();

    stronghold.spawn_stronghold_actor(client_path1.clone(), vec![]).await;

    stronghold.switch_actor_target(client_path0.clone()).await;

    assert!(stronghold
        .write_to_vault(
            loc0.clone(),
            b"test".to_vec(),
            RecordHint::new(b"0").expect(line_error!()),
            vec![],
        )
        .await
        .is_ok());

    // read head.
    let (p, _) = stronghold.read_secret(client_path0.clone(), loc0.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("test"));

    stronghold.switch_actor_target(client_path1.clone()).await;

    // Write on the next record of the vault using None.  This calls InitRecord and creates a new one at index 1.
    assert!(stronghold
        .write_to_vault(
            loc0.clone(),
            b"another test".to_vec(),
            RecordHint::new(b"1").expect(line_error!()),
            vec![],
        )
        .await
        .is_ok());

    // read head.
    let (p, _) = stronghold.read_secret(client_path1.clone(), loc0.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("another test"));

    stronghold.switch_actor_target(client_path0.clone()).await;

    assert!(stronghold
        .write_to_vault(
            loc0.clone(),
            b"yet another test".to_vec(),
            RecordHint::new(b"2").expect(line_error!()),
            vec![],
        )
        .await
        .is_ok());

    let (p, _) = stronghold.read_secret(client_path0.clone(), loc0.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("yet another test"));

    let (ids, _) = stronghold.list_hints_and_ids(loc2.vault_path()).await;
    println!("actor 0: {:?}", ids);

    stronghold
        .write_all_to_snapshot(&key_data.to_vec(), Some("megasnap".into()), None)
        .await;

    stronghold.switch_actor_target(client_path1.clone()).await;

    let (ids, _) = stronghold.list_hints_and_ids(loc2.vault_path()).await;
    println!("actor 1: {:?}", ids);

    stronghold.spawn_stronghold_actor(client_path2.clone(), vec![]).await;

    stronghold
        .read_snapshot(
            client_path2.clone(),
            Some(client_path1.clone()),
            &key_data,
            Some("megasnap".into()),
            None,
        )
        .await;

    // client_path2 correct?
    let (p, _) = stronghold.read_secret(client_path2.clone(), loc0).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("another test"));

    assert!(stronghold
        .write_to_vault(
            loc3.clone(),
            b"a new actor test".to_vec(),
            RecordHint::new(b"2").expect(line_error!()),
            vec![],
        )
        .await
        .is_ok());

    let (p, _) = stronghold.read_secret(client_path2.clone(), loc3).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("a new actor test"));

    assert!(stronghold
        .write_to_vault(
            loc4.clone(),
            b"a new actor test again".to_vec(),
            RecordHint::new(b"3").expect(line_error!()),
            vec![],
        )
        .await
        .is_ok());

    let (p, _) = stronghold.read_secret(client_path2, loc4.clone()).await;

    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("a new actor test again"));

    let (mut ids2, _) = stronghold.list_hints_and_ids(loc4.vault_path()).await;

    stronghold.switch_actor_target(client_path1).await;

    let (mut ids1, _) = stronghold.list_hints_and_ids(loc4.vault_path()).await;
    ids2.sort();
    ids1.sort();

    println!("ids and hints => actor 1: {:?}", ids1);
    println!("ids and hints => actor 2: {:?}", ids2);

    assert_eq!(ids1, ids2);

    stronghold.spawn_stronghold_actor(client_path3.clone(), vec![]).await;

    stronghold
        .read_snapshot(
            client_path3,
            Some(client_path0.clone()),
            &key_data,
            Some("megasnap".into()),
            None,
        )
        .await;

    let (mut ids3, _) = stronghold.list_hints_and_ids(loc4.vault_path()).await;
    println!("actor 3: {:?}", ids3);

    stronghold.switch_actor_target(client_path0).await;

    let (mut ids0, _) = stronghold.list_hints_and_ids(loc4.vault_path()).await;
    println!("actor 0: {:?}", ids0);

    ids0.sort();
    ids3.sort();

    assert_eq!(ids0, ids3);

    // stronghold.system.print_tree();
}

#[actix::test]
async fn test_stronghold_generics() {
    // let sys = ActorSystem::new().unwrap();
    let key_data = b"abcdefghijklmnopqrstuvwxyz012345".to_vec();

    let client_path = b"test a".to_vec();

    let slip10_seed = Location::generic("slip10", "seed");

    let mut stronghold = Stronghold::init_stronghold_system(client_path.clone(), vec![])
        .await
        .unwrap();

    assert!(stronghold
        .write_to_vault(
            slip10_seed.clone(),
            b"AAAAAA".to_vec(),
            RecordHint::new(b"first hint").expect(line_error!()),
            vec![],
        )
        .await
        .is_ok());
    let (p, _) = stronghold.read_secret(client_path, slip10_seed).await;
    assert_eq!(std::str::from_utf8(&p.unwrap()), Ok("AAAAAA"));

    stronghold
        .write_all_to_snapshot(&key_data.to_vec(), Some("generic".into()), None)
        .await;
}

/// this test has not been ported to actix
#[cfg(feature = "p2p")]
#[actix::test]
async fn test_stronghold_p2p() {
    use tokio::sync::{mpsc, oneshot};

    let system = actix::System::current();
    let arbiter = system.arbiter();

    let (addr_tx, addr_rx) = oneshot::channel();

    // Channel for signaling that local/ remote is ready i.g. performed a necessary write, before the other ran try
    // read.
    let (remote_ready_tx, mut remote_ready_rx) = mpsc::channel(1);
    let (local_ready_tx, mut local_ready_rx) = mpsc::channel(1);

    let loc1 = Location::counter::<_, usize>("path", 0);
    let data1 = b"some data".to_vec();
    let loc1_clone = loc1.clone();
    let data1_clone = data1.clone();

    let loc2 = Location::counter::<_, usize>("path", 1);
    let data2 = b"some second data".to_vec();
    let loc2_clone = loc2.clone();
    let data2_clone = data2.clone();

    let seed1 = fresh::location();
    let seed1_clone = seed1.clone();

    let (res_tx, mut res_rx) = mpsc::channel(1);
    let res_tx_clone = res_tx.clone();

    let spawned_local = arbiter.spawn(async move {
        let local_client = b"local".to_vec();
        let mut local_stronghold = Stronghold::init_stronghold_system(local_client, vec![])
            .await
            .unwrap_or_else(|e| panic!("Could not create a stronghold instance: {}", e));
        local_stronghold
            .spawn_p2p(Rule::AllowAll, NetworkConfig::default())
            .await;

        let (peer_id, addr) = addr_rx.await.unwrap();
        match local_stronghold.add_peer(peer_id, Some(addr), false, false).await {
            ResultMessage::Ok(_) => {}
            ResultMessage::Error(_) => panic!("Could not establish connection to remote."),
        }

        remote_ready_rx.recv().await.unwrap();

        // test writing at remote and reading it from local stronghold
        let payload = match local_stronghold.read_from_remote_store(peer_id, loc1).await {
            ResultMessage::Ok(payload) => payload,
            ResultMessage::Error(_) => panic!("Could not read from remote store."),
        };
        assert_eq!(payload, data1);

        // test writing from local and reading it at remote
        match local_stronghold.write_to_remote_store(peer_id, loc2, data2, None).await {
            StatusMessage::OK => {}
            StatusMessage::Error(_) => panic!("Could not write to remote store"),
        }
        local_ready_tx.send(()).await.unwrap();

        // test writing and reading from local
        let loc3 = Location::counter::<_, usize>("path", 2);
        let original_data3 = b"some third data".to_vec();
        match local_stronghold
            .write_to_remote_store(peer_id, loc3.clone(), original_data3.clone(), None)
            .await
        {
            StatusMessage::OK => {}
            StatusMessage::Error(_) => panic!("Could not write to remote store."),
        }
        let payload = match local_stronghold.read_from_remote_store(peer_id, loc3).await {
            ResultMessage::Ok(payload) => payload,
            ResultMessage::Error(_) => panic!("Could not read from remote store."),
        };
        assert_eq!(payload, original_data3);

        remote_ready_rx.recv().await.unwrap();

        let (_path, chain) = fresh::hd_path();
        let procedure = Procedure::SLIP10Derive {
            chain,
            input: SLIP10DeriveInput::Seed(seed1),
            output: fresh::location(),
            hint: fresh::record_hint(),
        };

        match local_stronghold.remote_runtime_exec(peer_id, procedure).await {
            ResultMessage::Ok(ProcResult::SLIP10Derive(ResultMessage::Ok(_))) => {}
            ResultMessage::Error(err) => panic!("Procedure failed: {:?}", err),
            r => panic!("unexpected result: {:?}", r),
        };
        res_tx.send(()).await.unwrap();
    });
    assert!(spawned_local);

    let spawned_remote = arbiter.spawn(async move {
        let remote_client = b"remote".to_vec();
        let mut remote_stronghold = Stronghold::init_stronghold_system(remote_client, vec![])
            .await
            .unwrap_or_else(|e| panic!("Could not create a stronghold instance: {}", e));
        remote_stronghold
            .spawn_p2p(Rule::AllowAll, NetworkConfig::default())
            .await;

        let addr = match remote_stronghold.start_listening(None).await {
            ResultMessage::Ok(addr) => addr,
            ResultMessage::Error(_) => panic!("Could not start listening"),
        };

        let (peer_id, listeners) = match remote_stronghold.get_swarm_info().await {
            ResultMessage::Ok(SwarmInfo {
                local_peer_id,
                listeners,
                ..
            }) => (local_peer_id, listeners),
            ResultMessage::Error(_) => panic!("Could not get swarm info."),
        };

        assert!(listeners.into_iter().any(|l| l.addrs.contains(&addr)));
        addr_tx.send((peer_id, addr)).unwrap();

        // test writing at remote and reading it from local stronghold
        match remote_stronghold.write_to_store(loc1_clone, data1_clone, None).await {
            StatusMessage::OK => {}
            StatusMessage::Error(_) => panic!("Could not write store."),
        };

        remote_ready_tx.send(()).await.unwrap();
        local_ready_rx.recv().await.unwrap();

        // test writing from local and reading it at remoteom local and reading it at remote
        let payload = match remote_stronghold.read_from_store(loc2_clone).await {
            (payload, StatusMessage::OK) => payload,
            (_, StatusMessage::Error(_)) => panic!("Could not read from store."),
        };
        assert_eq!(payload, data2_clone);

        // test procedure execution
        match remote_stronghold
            .runtime_exec(Procedure::SLIP10Generate {
                size_bytes: None,
                output: seed1_clone,
                hint: fresh::record_hint(),
            })
            .await
        {
            ProcResult::SLIP10Generate(ResultMessage::OK) => (),
            r => panic!("unexpected result: {:?}", r),
        };

        remote_ready_tx.send(()).await.unwrap();
        res_tx_clone.send(()).await.unwrap();
    });
    assert!(spawned_remote);

    // wait for both threads to return
    res_rx.recv().await.unwrap();
    res_rx.recv().await.unwrap();
}
