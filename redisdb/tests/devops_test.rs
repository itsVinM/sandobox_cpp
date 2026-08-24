use std::sync::Arc;

use tokio::sync::RwLock;

use redisops::handler;
use redisops::proto;
use redisops::store::Store;

#[tokio::test]
async fn test_job_submit_and_status() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));

    let resp = handler::dispatch(
        store.clone(),
        vec![
            "job submit".into(),
            "job-001".into(),
            "smoke-test".into(),
            "local".into(),
            "make test".into(),
        ],
        auth.clone(),
    )
    .await;
    assert_eq!(resp[0], proto::SER_STR);
    let len = u32::from_le_bytes(resp[1..5].try_into().unwrap()) as usize;
    let id = String::from_utf8(resp[5..5 + len].to_vec()).unwrap();
    assert_eq!(id, "job-001");

    let resp = handler::dispatch(
        store.clone(),
        vec!["job status".into(), "job-001".into()],
        auth.clone(),
    )
    .await;
    assert_eq!(resp[0], proto::SER_STR);
    let len = u32::from_le_bytes(resp[1..5].try_into().unwrap()) as usize;
    let status = String::from_utf8(resp[5..5 + len].to_vec()).unwrap();
    assert_eq!(status, "queued");
}

#[tokio::test]
async fn test_job_next() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));

    handler::dispatch(
        store.clone(),
        vec![
            "job submit".into(),
            "j1".into(),
            "test-a".into(),
            "local".into(),
            "run-a".into(),
        ],
        auth.clone(),
    )
    .await;
    handler::dispatch(
        store.clone(),
        vec![
            "job submit".into(),
            "j2".into(),
            "test-b".into(),
            "local".into(),
            "run-b".into(),
        ],
        auth.clone(),
    )
    .await;

    let resp = handler::dispatch(store.clone(), vec!["job next".into()], auth.clone()).await;
    assert_eq!(resp[0], proto::SER_ARR);
    let resp2 = handler::dispatch(store.clone(), vec!["job next".into()], auth.clone()).await;
    assert_eq!(resp2[0], proto::SER_ARR);
}

#[tokio::test]
async fn test_job_result() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));

    handler::dispatch(
        store.clone(),
        vec![
            "job submit".into(),
            "j1".into(),
            "test".into(),
            "local".into(),
            "make".into(),
        ],
        auth.clone(),
    )
    .await;

    let resp = handler::dispatch(
        store.clone(),
        vec!["job result".into(), "j1".into(), "0".into(), "1500".into()],
        auth.clone(),
    )
    .await;
    assert_eq!(resp[0], proto::SER_STR);
    let len = u32::from_le_bytes(resp[1..5].try_into().unwrap()) as usize;
    let status = String::from_utf8(resp[5..5 + len].to_vec()).unwrap();
    assert_eq!(status, "passed");
}

#[tokio::test]
async fn test_list_ops() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));

    handler::dispatch(
        store.clone(),
        vec!["lpush".into(), "q".into(), "a".into()],
        auth.clone(),
    )
    .await;
    handler::dispatch(
        store.clone(),
        vec!["lpush".into(), "q".into(), "b".into()],
        auth.clone(),
    )
    .await;

    let resp =
        handler::dispatch(store.clone(), vec!["llen".into(), "q".into()], auth.clone()).await;
    let n = i64::from_le_bytes(resp[1..9].try_into().unwrap());
    assert_eq!(n, 2);

    let resp =
        handler::dispatch(store.clone(), vec!["lpop".into(), "q".into()], auth.clone()).await;
    assert_eq!(resp[0], proto::SER_STR);
    let len = u32::from_le_bytes(resp[1..5].try_into().unwrap()) as usize;
    let val = String::from_utf8(resp[5..5 + len].to_vec()).unwrap();
    assert_eq!(val, "b");

    handler::dispatch(
        store.clone(),
        vec!["lpush".into(), "q".into(), "c".into()],
        auth.clone(),
    )
    .await;
    let resp = handler::dispatch(
        store.clone(),
        vec!["lrange".into(), "q".into(), "0".into(), "-1".into()],
        auth.clone(),
    )
    .await;
    assert_eq!(resp[0], proto::SER_ARR);
}

#[tokio::test]
async fn test_job_list() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));

    handler::dispatch(
        store.clone(),
        vec![
            "job submit".into(),
            "j1".into(),
            "t1".into(),
            "local".into(),
            "run".into(),
        ],
        auth.clone(),
    )
    .await;
    handler::dispatch(
        store.clone(),
        vec![
            "job submit".into(),
            "j2".into(),
            "t2".into(),
            "local".into(),
            "run".into(),
        ],
        auth.clone(),
    )
    .await;

    let resp = handler::dispatch(store.clone(), vec!["job list".into()], auth.clone()).await;
    assert_eq!(resp[0], proto::SER_ARR);
}
