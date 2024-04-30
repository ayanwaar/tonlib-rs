use std::fs::create_dir_all;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use futures::future::join_all;
use tokio::time::timeout;
use tokio::{self};
use tokio_test::assert_ok;
use tonlib::address::TonAddress;
use tonlib::cell::{key_extractor_256bit, value_extractor_cell, BagOfCells, GenericDictLoader};
use tonlib::client::{TonBlockFunctions, TonClient, TonClientBuilder, TonClientInterface, TxId};
use tonlib::config::{MAINNET_CONFIG, TESTNET_CONFIG};
use tonlib::contract::{TonContractFactory, TonContractInterface};
use tonlib::tl::{
    BlockId, BlockIdExt, BlocksShards, BlocksTransactions, BlocksTransactionsExt,
    InternalTransactionId, LiteServerInfo, SmcLibraryQueryExt, NULL_BLOCKS_ACCOUNT_TRANSACTION_ID,
};

mod common;

#[tokio::test]
async fn test_client_get_account_state_of_inactive() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await?;
    let factory = TonContractFactory::builder(&client).build().await?;
    for _ in 0..100 {
        let r = factory
            .get_latest_account_state(&TonAddress::from_base64_url(
                "EQDOUwuz-6lH-IL-hqSHQSrFhoNjTNjKp04Wb5n2nkctCJTH",
            )?)
            .await;
        log::info!("{:?}", r);
        assert!(r.is_ok());
        if r.unwrap().frozen_hash != Vec::<u8>::new() {
            panic!("Expected UnInited state")
        }
    }
    drop(factory);
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok(())
}

#[tokio::test]
async fn client_get_raw_account_state_works() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await?;
    let r = client
        .get_raw_account_state(&TonAddress::from_base64_url(
            "EQDk2VTvn04SUKJrW7rXahzdF8_Qi6utb0wj43InCu9vdjrR",
        )?)
        .await;
    println!("{:?}", r);
    assert!(r.is_ok());
    Ok(())
}

#[tokio::test]
async fn client_get_raw_transactions_works() -> anyhow::Result<()> {
    common::init_logging();
    let address = &TonAddress::from_base64_url("EQDk2VTvn04SUKJrW7rXahzdF8_Qi6utb0wj43InCu9vdjrR")?;
    let max_retries = 3;
    let mut retries = 0;
    while retries < max_retries {
        retries += 1;
        let client = common::new_mainnet_client().await?;
        let state = client.get_raw_account_state(address).await.unwrap();
        let r = client
            .get_raw_transactions(address, &state.last_transaction_id)
            .await;
        println!("{:?}", r);
        if r.is_ok() {
            let cnt = 1;
            let r = client
                .get_raw_transactions_v2(address, &state.last_transaction_id, cnt, false)
                .await;
            println!("{:?}", r);
            if r.is_ok() {
                assert_eq!(r.unwrap().transactions.len(), cnt);
                return Ok(());
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn client_smc_run_get_method_works() -> anyhow::Result<()> {
    common::init_logging();
    {
        let client = common::new_mainnet_client().await?;
        let address =
            &TonAddress::from_base64_url("EQDk2VTvn04SUKJrW7rXahzdF8_Qi6utb0wj43InCu9vdjrR")?;
        let loaded_state = client.smc_load(address).await?; // pool 0.3.0
        let method_id = "get_jetton_data".into();
        let conn = loaded_state.conn.clone();

        let r = loaded_state
            .conn
            .smc_run_get_method(loaded_state.id, &method_id, &Vec::new())
            .await;
        println!("{:?}", r);
        // Check that it works after cloning the connection
        let id2 = {
            let conn2 = conn.clone();
            conn2
                .smc_load(address)
                .await? // pool 0.3.0
                .id
        };
        let stack = &Vec::new();
        let future = conn.smc_run_get_method(id2, &method_id, stack);
        let r = timeout(Duration::from_secs(2), future).await?;
        println!("{:?}", r);
    }
    thread::sleep(Duration::from_secs(2));
    Ok(())
}

#[tokio::test]
async fn client_smc_load_by_transaction_works() -> anyhow::Result<()> {
    common::init_logging();

    let address = &TonAddress::from_base64_url("EQCVx4vipWfDkf2uNhTUkpT97wkzRXHm-N1cNn_kqcLxecxT")?;
    let internal_transaction_id = InternalTransactionId::from_str(
        "32016630000001:91485a21ba6eaaa91827e357378fe332228d11f3644e802f7e0f873a11ce9c6f",
    )?;

    let max_retries = 3;
    let mut retries = 0;
    while retries < max_retries {
        retries += 1;
        let client = common::new_mainnet_client().await?;

        let state = client.get_raw_account_state(address).await.unwrap();

        println!("TRANSACTION_ID{}", &state.last_transaction_id);

        let tx_id = Arc::new(TxId {
            address: address.clone(),
            internal_transaction_id: internal_transaction_id.clone(),
        });
        let res = client
            .smc_load_by_transaction(&tx_id.address, &tx_id.internal_transaction_id)
            .await;

        if res.is_ok() {
            return Ok(());
        }
    }

    Ok(())
}

#[tokio::test]
async fn client_smc_get_code_works() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await?;
    let address = &TonAddress::from_base64_url("EQDk2VTvn04SUKJrW7rXahzdF8_Qi6utb0wj43InCu9vdjrR")?;
    let loaded_state = client.smc_load(address).await?;
    let cell = loaded_state.conn.smc_get_code(loaded_state.id).await?;
    println!("\n\r\x1b[1;35m-----------------------------------------CODE-----------------------------------------\x1b[0m:\n\r {:?}", STANDARD.encode(cell.bytes));
    Ok(())
}

#[tokio::test]
async fn client_smc_get_data_works() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await?;
    let address = &TonAddress::from_base64_url("EQDk2VTvn04SUKJrW7rXahzdF8_Qi6utb0wj43InCu9vdjrR")?;
    let loaded_state = client.smc_load(address).await?;
    let cell = loaded_state.conn.smc_get_data(loaded_state.id).await?;
    println!("\n\r\x1b[1;35m-----------------------------------------DATA-----------------------------------------\x1b[0m:\n\r {:?}", STANDARD.encode(cell.bytes));
    Ok(())
}

#[tokio::test]
async fn test_get_jetton_content_internal_uri_jusdt() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await?;
    let address = &TonAddress::from_base64_url("EQDk2VTvn04SUKJrW7rXahzdF8_Qi6utb0wj43InCu9vdjrR")?;
    let loaded_state = client.smc_load(address).await?;
    let cell = loaded_state.conn.smc_get_state(loaded_state.id).await?;
    println!("\n\r\x1b[1;35m-----------------------------------------STATE----------------------------------------\x1b[0m:\n\r {:?}", cell);
    Ok(())
}

#[tokio::test]
async fn client_get_block_header_works() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await?;
    let (_, info) = client.get_masterchain_info().await?;
    let seqno = info.last.seqno;
    let block_id = BlockId {
        workchain: -1,
        shard: i64::MIN,
        seqno,
    };
    let block_id_ext = client.lookup_block(1, &block_id, 0, 0).await?;
    let r = client.get_block_header(&block_id_ext).await?;
    println!("{:?}", r);
    Ok(())
}

#[tokio::test]
async fn test_client_blocks_get_transactions() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await?;
    let (_, info) = client.get_masterchain_info().await?;
    log::info!("MasterchainInfo: {:?}", &info);
    let block_id = BlockId {
        workchain: info.last.workchain,
        shard: info.last.shard,
        seqno: info.last.seqno,
    };
    let block_id_ext = client.lookup_block(1, &block_id, 0, 0).await?;
    log::info!("BlockIdExt: {:?}", &block_id_ext);
    let block_shards: BlocksShards = client.get_block_shards(&info.last).await?;
    let mut shards = block_shards.shards.clone();
    log::info!("Shards: {:?}", &block_shards);
    shards.insert(0, info.last.clone());
    for shard in &shards {
        log::info!("Processing shard: {:?}", shard);
        let workchain = shard.workchain;
        let txs: BlocksTransactions = client
            .get_block_transactions(shard, 7, 1024, &NULL_BLOCKS_ACCOUNT_TRANSACTION_ID)
            .await?;
        log::info!(
            "Number of transactions: {}, incomplete: {}",
            txs.transactions.len(),
            txs.incomplete
        );
        for tx_id in txs.transactions {
            let mut t: [u8; 32] = [0; 32];
            t.clone_from_slice(tx_id.account.as_slice());
            let addr = TonAddress::new(workchain, &t);
            let id = InternalTransactionId {
                hash: tx_id.hash.clone(),
                lt: tx_id.lt,
            };
            let tx = client.get_raw_transactions_v2(&addr, &id, 1, false).await?;
            log::info!("Tx: {:?}", tx.transactions[0])
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_client_blocks_get_transactions_ext() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await?;
    let (_, info) = client.get_masterchain_info().await?;
    log::info!("MasterchainInfo: {:?}", &info);
    let block_id = BlockId {
        workchain: info.last.workchain,
        shard: info.last.shard,
        seqno: info.last.seqno,
    };
    let block_id_ext = client.lookup_block(1, &block_id, 0, 0).await?;
    log::info!("BlockIdExt: {:?}", &block_id_ext);
    let block_shards: BlocksShards = client.get_block_shards(&info.last).await?;
    let mut shards = block_shards.shards.clone();
    log::info!("Shards: {:?}", &block_shards);
    shards.insert(0, info.last.clone());
    for shard in &shards {
        log::info!("Processing shard: {:?}", shard);
        let txs: BlocksTransactionsExt = client
            .get_block_transactions_ext(shard, 7, 1024, &NULL_BLOCKS_ACCOUNT_TRANSACTION_ID)
            .await?;
        log::info!(
            "Number of transactions: {}, incomplete: {}",
            txs.transactions.len(),
            txs.incomplete
        );
        for raw_tx in txs.transactions {
            let addr = TonAddress::from_base64_url(raw_tx.address.account_address.as_str())?;
            let id = raw_tx.transaction_id;
            let tx = client.get_raw_transactions_v2(&addr, &id, 1, false).await?;
            log::info!("Tx: {:?}", tx.transactions[0])
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_client_lite_server_get_info() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_testnet_client().await?;
    let info: LiteServerInfo = client.lite_server_get_info().await?;

    println!("{:?}", info);
    Ok(())
}

#[tokio::test]
async fn test_get_config_param() -> anyhow::Result<()> {
    common::init_logging();
    let client = &common::new_testnet_client().await?;
    let info = client.get_config_param(0u32, 34u32).await?;
    let config_data = info.config.bytes;
    let bag = BagOfCells::parse(config_data.as_slice())?;
    let config_cell = bag.single_root()?;
    let mut parser = config_cell.parser();
    let n = parser.load_u8(8)?;
    assert!(n == 0x12u8);
    Ok(())
}

#[tokio::test]
pub async fn test_get_block_header() -> anyhow::Result<()> {
    common::init_logging();
    let client = &common::new_testnet_client().await?;
    let (_, info) = client.get_masterchain_info().await?;
    let seqno = info.last;
    let headers = client.get_block_header(&seqno).await?;
    println!("{:?}", headers);
    Ok(())
}

#[tokio::test]
async fn test_get_shard_tx_ids() -> anyhow::Result<()> {
    common::init_logging();
    let client = &common::new_testnet_client().await?;
    let (_, info) = client.get_masterchain_info().await?;
    let shards = client.get_block_shards(&info.last).await?;
    assert!(!shards.shards.is_empty());
    let ids = client.get_shard_tx_ids(&shards.shards[0]).await?;
    println!("{:?}", ids);
    Ok(())
}

#[tokio::test]
async fn test_get_shard_transactions_works() -> anyhow::Result<()> {
    common::init_logging();
    let client = &assert_ok!(common::new_testnet_client().await);
    let (_, info) = assert_ok!(client.get_masterchain_info().await);
    let shards = assert_ok!(client.get_block_shards(&info.last).await);
    assert!(!shards.shards.is_empty());
    let txs = assert_ok!(client.get_shard_transactions(&shards.shards[0]).await);
    assert!(txs.len() > 0);
    println!("{:?}", txs);
    Ok(())
}

#[tokio::test]
async fn test_get_shard_transactions_parse_address_correctly() -> anyhow::Result<()> {
    common::init_logging();
    let client = &assert_ok!(common::new_mainnet_client().await);
    assert_ok!(client.sync().await);
    // manually selected block with particular addresses format in transactions
    let block_shard = BlockIdExt {
        workchain: 0,
        shard: -4611686018427387904,
        seqno: 43256197,
        root_hash: "yEteKr1hD3d20O/ZL+Y7AB2YD9xL1NZ9r0fXPwYlbYA=".to_string(),
        file_hash: "VrzW8+EtGDYiaSiYQEou9N5+YWF2CeBzxmAMXUOZ5mE=".to_string(),
    };
    let txs = assert_ok!(client.get_shard_transactions(&block_shard).await);
    assert!(txs.len() > 0);
    println!("{:?}", txs);
    Ok(())
}

#[tokio::test]
async fn test_get_shards_transactions() -> anyhow::Result<()> {
    common::init_logging();
    let client = &common::new_testnet_client().await?;
    let (_, info) = client.get_masterchain_info().await?;
    let shards = client.get_block_shards(&info.last).await?;
    assert!(!shards.shards.is_empty());
    let shards_txs = client.get_shards_transactions(&shards.shards).await?;
    for s in shards_txs {
        println!("{:?} : {:?}", s.0, s.1);
    }
    Ok(())
}

#[tokio::test]
async fn test_missing_block_error() -> anyhow::Result<()> {
    common::init_logging();
    let client = &common::new_testnet_client().await?;
    let (_, info) = client.get_masterchain_info().await?;
    let block_id = BlockId {
        workchain: info.last.workchain,
        shard: info.last.shard,
        seqno: info.last.seqno + 2,
    };
    for _i in 0..100 {
        let res = client.lookup_block(1, &block_id, 0, 0).await;
        log::info!("{:?}", res);
        tokio::time::sleep(Duration::from_millis(100)).await;
        if res.is_ok() {
            break;
        };
    }
    Ok(())
}

#[tokio::test]
async fn test_first_block_error() -> anyhow::Result<()> {
    common::init_logging();
    let client = &common::new_archive_testnet_client().await?;
    let (_, info) = client.get_masterchain_info().await?;
    let block_id = BlockId {
        workchain: info.last.workchain,
        shard: info.last.shard,
        seqno: 1,
    };
    let res = client.lookup_block(1, &block_id, 0, 0).await;
    log::info!("{:?}", res);

    Ok(())
}

#[tokio::test]
async fn test_keep_connection_alive() -> anyhow::Result<()> {
    common::init_logging();
    let client = &common::new_archive_testnet_client().await?;
    let (_, info) = client.get_masterchain_info().await?;
    let next_block_id = BlockId {
        workchain: info.last.workchain,
        shard: info.last.shard,
        seqno: info.last.seqno + 10,
    };
    let first_block_id = BlockId {
        workchain: -1,
        shard: i64::MIN,
        seqno: 1,
    };
    let conn = client.get_connection().await?;
    let r1 = conn.lookup_block(1, &first_block_id, 0, 0).await;
    log::info!("R1: {:?}", r1);
    let r2 = conn.lookup_block(1, &next_block_id, 0, 0).await;
    log::info!("R1: {:?}", r2);
    let r3 = conn.lookup_block(1, &first_block_id, 0, 0).await;
    log::info!("R1: {:?}", r3);
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok(())
}

#[tokio::test]
async fn client_mainnet_works() -> anyhow::Result<()> {
    common::init_logging();
    let client = TonClient::builder()
        .with_pool_size(2)
        .with_config(MAINNET_CONFIG)
        .build()
        .await?;
    let (_, info) = client.get_masterchain_info().await?;
    let shards = client.get_block_shards(&info.last).await?;
    let blocks_header = client.get_block_header(&info.last).await?;
    assert!(!shards.shards.is_empty());
    let shards_txs = client.get_shards_transactions(&shards.shards).await?;
    for s in shards_txs {
        log::info!(" BlockId: {:?}\n Transactions: {:?}", s.0, s.1.len());
    }
    log::info!(
        "MAINNET: Blocks header for  {} seqno : {:?}",
        info.last.seqno,
        blocks_header
    );
    Ok(())
}

#[tokio::test]
async fn client_testnet_works() -> anyhow::Result<()> {
    common::init_logging();
    let client = TonClient::builder()
        .with_pool_size(2)
        .with_config(TESTNET_CONFIG)
        .build()
        .await?;
    let (_, info) = client.get_masterchain_info().await?;
    let shards = client.get_block_shards(&info.last).await?;
    assert!(!shards.shards.is_empty());
    let shards_txs = client.get_shards_transactions(&shards.shards).await?;
    let blocks_header = client.get_block_header(&info.last).await?;
    for s in shards_txs {
        log::info!(" BlockId: {:?}\n Transactions: {:?}", s.0, s.1);
    }

    log::info!(
        "TESTNET: Blocks header for  {} seqno : {:?}",
        info.last.seqno,
        blocks_header
    );
    Ok(())
}

#[tokio::test]
async fn client_smc_get_libraries() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await?;
    let library_hash_str = "TwFxJywhW4v4/urEaoV2iKS2X0/mH4IoYx9ifQ7anQA=";

    let library_list = &[library_hash_str.to_string()];
    let smc_library_result = client.smc_get_libraries(library_list).await?;

    log::info!(
        "smc_library_result {:?}",
        STANDARD.encode(smc_library_result.result[0].hash.clone())
    );
    assert_eq!(
        STANDARD.encode(smc_library_result.result[0].hash.clone()),
        library_hash_str
    );

    // we just test that library code is a valid boc:
    let boc = BagOfCells::parse(smc_library_result.result[0].data.as_slice())?;
    log::info!("smc_library_result {:?}", boc);

    Ok(())
}

#[tokio::test]
async fn client_smc_get_libraries_ext() -> anyhow::Result<()> {
    common::init_logging();

    let client = common::new_mainnet_client().await?;

    let address = TonAddress::from_base64_url("EQDqVNU7Jaf85MhIba1lup0F7Mr3rGigDV8RxMS62RtFr1w8")?; //jetton master
    let factory = TonContractFactory::builder(&client).build().await?;
    let contract = factory.get_contract(&address);
    let code = &contract.get_account_state().await?.code;
    let library_query = SmcLibraryQueryExt::ScanBoc {
        boc: code.clone(),
        max_libs: 10,
    };

    let library_hash = "TwFxJywhW4v4/urEaoV2iKS2X0/mH4IoYx9ifQ7anQA=";

    let smc_libraries_ext_result = client.smc_get_libraries_ext(vec![library_query]).await?;

    log::info!("smc_libraries_ext_result {:?}", smc_libraries_ext_result);

    assert_eq!(1, smc_libraries_ext_result.libs_ok.len());
    assert_eq!(0, smc_libraries_ext_result.libs_not_found.len());
    assert_eq!(smc_libraries_ext_result.libs_ok[0].as_str(), library_hash);

    let boc = BagOfCells::parse(&smc_libraries_ext_result.dict_boc)?;
    let cell = boc.single_root()?;
    let dict_loader = GenericDictLoader::new(key_extractor_256bit, value_extractor_cell, 256);
    let dict = cell.load_generic_dict(&dict_loader)?;

    log::info!("DICT: {:?}", dict);

    assert_eq!(dict.len(), 1);
    assert!(dict.contains_key(STANDARD.decode(library_hash)?.as_slice()));

    Ok(())
}

// This test fails on tonlib 2023.6, 2024.1 and 2024.3 either with:
//   error: test failed, to rerun pass `-p tonlib --test client_test`
//     Caused by:
//     process didn't exit successfully: `../target/debug/deps/client_test-a6ec52f42b3d3962 dropping_invoke_test --exact --nocapture --ignored`
//    (signal: 6, SIGABRT: process abort signal)
//  or:
//   error: test failed, to rerun pass `-p tonlib --test client_test`
//     Caused by:
//     process didn't exit successfully: `../target/debug/deps/client_test-a6ec52f42b3d3962 dropping_invoke_test --exact --nocapture --ignored`
//     (signal: 11, SIGSEGV: invalid memory reference)
#[ignore]
#[tokio::test]
async fn dropping_invoke_test() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await?;
    let address = TonAddress::from_base64_url("EQDk2VTvn04SUKJrW7rXahzdF8_Qi6utb0wj43InCu9vdjrR")?;
    client.get_raw_account_state(&address).await?;

    let f = [
        abort_batch_invoke_get_raw_account_state(&client, Duration::from_millis(100)),
        abort_batch_invoke_get_raw_account_state(&client, Duration::from_millis(200)),
        abort_batch_invoke_get_raw_account_state(&client, Duration::from_millis(500)),
    ];

    join_all(f).await;

    Ok(())
}

async fn abort_batch_invoke_get_raw_account_state(
    client: &TonClient,
    dt: Duration,
) -> anyhow::Result<()> {
    let address = TonAddress::from_base64_url("EQDk2VTvn04SUKJrW7rXahzdF8_Qi6utb0wj43InCu9vdjrR")?;
    let addresses = vec![address; 100];

    let futures = addresses
        .iter()
        .map(|a| timeout(dt, client.get_raw_account_state(a)))
        .collect::<Vec<_>>();

    let result = join_all(futures).await;

    let res = result.iter().map(|r| r.is_ok()).collect::<Vec<_>>();
    log::info!("{:?}", res);

    Ok(())
}

#[tokio::test]
async fn archive_node_client_test() -> anyhow::Result<()> {
    let tonlib_work_dir = "./var/tonlib";
    create_dir_all(Path::new(tonlib_work_dir)).unwrap();
    TonClient::set_log_verbosity_level(2);

    let mut client_builder = TonClientBuilder::new();
    client_builder
        .with_config(MAINNET_CONFIG)
        .with_keystore_dir(String::from(tonlib_work_dir))
        .with_connection_check(tonlib::client::ConnectionCheck::Archive);
    let client = client_builder.build().await.unwrap();
    let (_, master_info) = client.get_masterchain_info().await.unwrap();
    println!("master_info: {:?}", master_info);
    Ok(())
}
