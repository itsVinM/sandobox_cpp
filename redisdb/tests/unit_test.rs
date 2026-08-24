use std::sync::Arc;

use tokio::sync::RwLock;

use redisops::handler;
use redisops::proto;
use redisops::store::Store;

#[tokio::test]
async fn test_handler_set_get() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));

    let resp = handler::dispatch(
        store.clone(),
        vec!["set".into(), "k".into(), "v".into()],
        auth.clone(),
    )
    .await;
    assert_eq!(resp[0], proto::SER_NIL);

    let resp = handler::dispatch(store.clone(), vec!["get".into(), "k".into()], auth.clone()).await;
    assert_eq!(resp[0], proto::SER_STR);
    let len = u32::from_le_bytes(resp[1..5].try_into().unwrap()) as usize;
    let val = String::from_utf8(resp[5..5 + len].to_vec()).unwrap();
    assert_eq!(val, "v");
}

#[tokio::test]
async fn test_handler_del() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));

    handler::dispatch(
        store.clone(),
        vec!["set".into(), "k".into(), "v".into()],
        auth.clone(),
    )
    .await;
    let resp = handler::dispatch(store.clone(), vec!["del".into(), "k".into()], auth.clone()).await;
    assert_eq!(resp[0], proto::SER_INT);
    let n = i64::from_le_bytes(resp[1..9].try_into().unwrap());
    assert_eq!(n, 1);

    let resp = handler::dispatch(store.clone(), vec!["del".into(), "k".into()], auth.clone()).await;
    let n = i64::from_le_bytes(resp[1..9].try_into().unwrap());
    assert_eq!(n, 0);
}

#[tokio::test]
async fn test_handler_zadd_zscore() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));

    let resp = handler::dispatch(
        store.clone(),
        vec!["zadd".into(), "z".into(), "3.25".into(), "pi".into()],
        auth.clone(),
    )
    .await;
    assert_eq!(resp[0], proto::SER_INT);
    let n = i64::from_le_bytes(resp[1..9].try_into().unwrap());
    assert_eq!(n, 1);

    let resp = handler::dispatch(
        store.clone(),
        vec!["zscore".into(), "z".into(), "pi".into()],
        auth.clone(),
    )
    .await;
    assert_eq!(resp[0], proto::SER_DBL);
    let val = f64::from_le_bytes(resp[1..9].try_into().unwrap());
    assert!((val - 3.25).abs() < 1e-9);
}

#[tokio::test]
async fn test_handler_unknown() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));
    let resp = handler::dispatch(store.clone(), vec!["nosuch".into()], auth.clone()).await;
    assert_eq!(resp[0], proto::SER_ERR);
}

#[tokio::test]
async fn test_handler_arity() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));
    let resp = handler::dispatch(store.clone(), vec!["get".into()], auth.clone()).await;
    assert_eq!(resp[0], proto::SER_ERR);
}

#[tokio::test]
async fn test_handler_list_ops() {
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
}

#[tokio::test]
async fn test_handler_job_queue() {
    let store = Store::new();
    let auth = Arc::new(RwLock::new(handler::AuthConfig::default()));

    let resp = handler::dispatch(
        store.clone(),
        vec![
            "job submit".into(),
            "j1".into(),
            "test".into(),
            "local".into(),
            "echo hi".into(),
        ],
        auth.clone(),
    )
    .await;
    assert_eq!(resp[0], proto::SER_STR);

    let resp = handler::dispatch(store.clone(), vec!["job next".into()], auth.clone()).await;
    assert_eq!(resp[0], proto::SER_ARR);

    let resp = handler::dispatch(
        store.clone(),
        vec!["job result".into(), "j1".into(), "0".into(), "100".into()],
        auth.clone(),
    )
    .await;
    assert_eq!(resp[0], proto::SER_STR);
    let len = u32::from_le_bytes(resp[1..5].try_into().unwrap()) as usize;
    let status = String::from_utf8(resp[5..5 + len].to_vec()).unwrap();
    assert_eq!(status, "passed");
}
