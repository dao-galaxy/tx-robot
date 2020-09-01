
mod config;
use clap::{App, load_yaml, ArgMatches, Arg};
use common_types::ipc::{IpcRequest, IpcReply, query_account_info};
use common_types::transaction::{UnverifiedTransaction};
use zmq::{Socket, Context, DEALER, ROUTER, DONTWAIT};
use hex_literal::hex;
use rlp::{Decodable, DecoderError, Encodable, Rlp, RlpStream};
use std::str::FromStr; // !!! Necessary for H160::from_str(address).expect("...");
use ethereum_types::{H160, H512, H256, U256};
use serde_derive::{Deserialize, Serialize};
use log::info;
use std::env;
use env_logger;
use config::*;
use rand::{thread_rng, Rng};
use rand::prelude::ThreadRng;
use std::time::{Duration, SystemTime};
use std::thread::sleep;

fn main() {
    // The YAML file is found relative to the current file, similar to how modules are found
    let yaml = load_yaml!("clap.yaml");  // src/clap.yaml
    let matches = App::from(yaml).get_matches();

    let config_file = matches.value_of("config").unwrap_or("src/bloom.conf");
    let toml_string = read_config_file(config_file);
    let decoded_config = parse_config_string(toml_string.as_str());
    let decoded_config_clone = decoded_config.clone();

    let mut log_level = matches.value_of("log").unwrap_or(
        &decoded_config.log_level.unwrap_or("debug".to_string())
    ).to_string();

    env::set_var("RUST_LOG", log_level.as_str());
    env_logger::init();
    info!("log level: {:?}", log_level.as_str());
    info!("{:#?}", matches);
    info!("{:#?}", decoded_config_clone);

    let query_socket = decoded_config.query_socket.unwrap_or("tcp://127.0.0.1:9050".to_string());
    let txpool_socket = decoded_config.txpool_socket.unwrap_or("tcp://127.0.0.1:7050".to_string());

    let mut accounts = accounts_vec();
    send_random_tx(
        &mut accounts,
        query_socket.as_str(),
        txpool_socket.as_str(),
        decoded_config.period.unwrap_or(3),
    );
}

fn random_pair(rng: &mut ThreadRng, low: usize, high: usize) -> (usize, usize) {
    let mut foo = 0;
    let mut bar = 0;
    while (foo == bar) {
        foo = rng.gen_range(low, high);
        bar = rng.gen_range(low, high);
    }
    (foo , bar)
}

fn generate_tx_mode1(query_socket: &Socket, sender: &Account, receiver: &Account) -> Vec<u8> {
    let receiver_address = H160::from_str(receiver.address.as_str()).unwrap();
    let tx_ret = sign_tx(
        &query_socket,
        sender,
        1,
        Some(receiver_address),
        U256::from(100000),
        U256::from(10),
        U256::from(50000),
        hex::decode("123456").unwrap(),
    );
    tx_ret
}

fn generate_tx_mode2(query_socket: &Socket, sender: &Account, receiver: &Account) -> Vec<u8> {
    let receiver_address = H160::from_str(receiver.address.as_str()).unwrap();
    let tx_ret = sign_tx(
        &query_socket,
        sender,
        1,
        Some(receiver_address),
        U256::from(100),
        U256::from(10),
        U256::from(50000),
        hex::decode("123456").unwrap(),
    );
    tx_ret
}

fn send_random_tx(
    accounts: &mut Vec<Account>,
    query_socket_str: &str,
    txpool_socket_str: &str,
    period: u64
) {
    let context = Context::new();
    let txpool_socket = context.socket(DEALER).unwrap();
    txpool_socket.set_identity( &hex!("1234").to_vec() ).unwrap();
    txpool_socket.connect(txpool_socket_str).unwrap();

    let query_socket = context.socket(DEALER).unwrap();
    query_socket.set_identity( &hex!("1234").to_vec() ).unwrap();
    query_socket.connect(query_socket_str).unwrap();

    let mut rng = thread_rng();
    let mut count : u128 = 0;
    loop {
        let (foo, bar) = random_pair(&mut rng, 1, 10);
        let mut zee = (count % 9 + 1) as usize;
        count += 1;

        info!("\n\n####account[0] => account[{}]].", zee);
        let tx = generate_tx_mode1(&query_socket, &accounts[0], &accounts[zee]);
        info!("{}", hex::encode(&tx).as_str());
        let mut tx_vec = vec![];
        tx_vec.push(&tx);
        send_to_txpool(&txpool_socket, &tx_vec, period);

        info!("\n\n####account[{}] => account[{}].", foo, bar);
        let tx = generate_tx_mode2(&query_socket, &accounts[foo], &accounts[bar]);
        info!("{}", hex::encode(&tx).as_str());
        let mut tx_vec = vec![];
        tx_vec.push(&tx);
        send_to_txpool(&txpool_socket, &tx_vec, period);

        /*
        info!("\n\n####account[8] => account[3].");
        let tx = generate_tx(&query_socket, &accounts[8], &accounts[3]);
        let mut tx_vec = vec![];
        tx_vec.push(&tx);
        send_to_txpool(&txpool_socket, &tx_vec, period);
        */

    }
}


fn send_to_txpool(socket: &Socket, tx_vec: &Vec<&Vec<u8>>, sleep_secs: u64) {
    let mut uv_tx_vec = vec![];
    for tx in tx_vec {
        let uv_tx: UnverifiedTransaction = rlp::decode(tx).unwrap();
        info!("{:?}", uv_tx);
        uv_tx_vec.push(uv_tx);
    }

    let param_bytes = rlp::encode_list(&uv_tx_vec);

    let secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let ipc_request = IpcRequest {
        method: "SendToTxPool".to_string(),
        id: secs,
        params: param_bytes,
    };
    // let recovered_request : IpcRequest = rlp::decode(&ipc_request.rlp_bytes()).unwrap();
    // info!("****Recovered request: {:x?}", recovered_request);

    socket.send(ipc_request.rlp_bytes(), 0).unwrap();
    let result_rmp;

    if sleep_secs == 0 {
        result_rmp = socket.recv_multipart(0);
    } else {
        sleep(Duration::from_secs(sleep_secs));
        result_rmp = socket.recv_multipart(DONTWAIT);
    }

    if let Ok(mut rmp) = result_rmp {
        let reply: IpcReply = rlp::decode(&rmp.pop().unwrap()).unwrap();
        info!("****IpcReply decoded: {:?}", reply);
        let result : String = rlp::decode(&reply.result).unwrap();
        info!("****Result decoded: {:?}", result);
        assert_eq!(reply.id, secs);
    } else {
        info!("****Error: Reply Timeout or Terminated Unexpectedly!");
    }
}


fn sign_tx(
    query_socket: &Socket,
    account: &Account,
    chain_id: u32,
    to: Option<H160>,
    value: U256,
    gas_price: U256,
    gas: U256,
    data: Vec<u8>
) -> Vec<u8> {
    let from = H160::from_str(account.address.as_str()).unwrap();
    let (nonce, balance) = query_account_info(&query_socket, &from);
    let tx = ethereum_tx_sign::RawTransaction {
        nonce,
        to,
        value,
        gas_price,
        gas,
        data,
    };
    let private_key = H256::from_str(&account.secret.as_str()).unwrap();
    let raw_rlp_bytes = tx.sign(&private_key, &chain_id);
    raw_rlp_bytes
}


#[test]
fn test_sign_tx() {
    // 1 mainnet, 3 ropsten
    const ETH_CHAIN_ID: u32 = 3;

    let tx = ethereum_tx_sign::RawTransaction {
        nonce: web3::types::U256::from(0),
        to: Some(web3::types::H160::zero()),
        value: web3::types::U256::zero(),
        gas_price: web3::types::U256::from(10000),
        gas: web3::types::U256::from(21240),
        data: hex::decode(
            "7f7465737432000000000000000000000000000000000000000000000000000000600057"
        ).unwrap(),
    };

    let mut data: [u8; 32] = Default::default();
    data.copy_from_slice(&hex::decode(
        "2a3526dd05ad2ebba87673f711ef8c336115254ef8fcd38c4d8166db9a8120e4"
    ).unwrap());
    let private_key = web3::types::H256(data);
    let raw_rlp_bytes = tx.sign(&private_key, &ETH_CHAIN_ID);

    let result = "f885808227108252f894000000000000000000000000000000000000000080a\
    47f746573743200000000000000000000000000000000000000000000000000\
    00006000572aa0b4e0309bc4953b1ca0c7eb7c0d15cc812eb4417cbd759aa09\
    3d38cb72851a14ca036e4ee3f3dbb25d6f7b8bd4dac0b4b5c717708d20ae6ff\
    08b6f71cbf0b9ad2f4";
    assert_eq!(result, hex::encode(raw_rlp_bytes));
}


#[test]
fn test_send_tx() {
    use zmq::{Context, DEALER, ROUTER, DONTWAIT};
    use std::time::Duration;
    use hex_literal::hex;
    use rlp::{Decodable, DecoderError, Encodable, Rlp, RlpStream};

    let txfoo = "f86e808227108252d094fff7e25dff2aa60f61f9d98130c8646a01f3164989055de6a779bbac00008455aa66cc25a0d42ff575cc734cabc779536c97160592d46c3518583f55823858b49a226d5a24a054b1c00c57421428f256d765b1d1e9739a6343d82e4a7800591338223d238e3d";
    let txbar = "f86a018227108252d09400cf3711cbd3a1512570639280758118ba0b2bcb8904563918244f4000008025a0b387e98ea78f840f04c6298db3322cc2db192a3fe2d6b0d267ef504ace3e566ea07101dcba98875d024d3a3f266465220205c8e3c4364e3386f642c8c562d07ddf";
    let txzee = "f86a028227108252d09400cf3711cbd3a1512570639280758118ba0b2bcb8904563918244f4000008026a01c3d3877e4989207353e8e2748bfb9632f82cb1c5e5a695b5fa29774bd8ee0a2a00dcf63efb7a151a957440e528555b56a27267b7224c01df95a47791a70e98c2e";

    let foo : UnverifiedTransaction = rlp::decode(&hex::decode(txfoo).unwrap()).unwrap();
    let bar : UnverifiedTransaction = rlp::decode(&hex::decode(txbar).unwrap()).unwrap();
    let zee : UnverifiedTransaction = rlp::decode(&hex::decode(txzee).unwrap()).unwrap();
    // let foobar_vec = vec![foo, bar];
    let uv_tx_vec = vec![foo, bar, zee];
    let rlp_bytes = rlp::encode_list(&uv_tx_vec);

    let ipc_request = IpcRequest {
        method: "SendToTxPool".to_string(),
        id: 666,
        params: rlp_bytes,
    };
    let recovered_request : IpcRequest = rlp::decode(&ipc_request.rlp_bytes()).unwrap();
    println!("Recovered request: {:x?}", recovered_request);

    // let socket = Context::new().socket(DEALER).unwrap();
    let context = Context::new();
    let socket = context.socket(DEALER).unwrap();
    socket.set_identity( &hex!("1234").to_vec() ).unwrap();
    //socket.connect("tcp://203.195.218.114:7050").unwrap();
    socket.connect("tcp://127.0.0.1:7050").unwrap();
    // socket.connect("tcp://172.31.253.119:7050").unwrap();
    // socket.connect("tcp://47.57.153.243:7050").unwrap();
    socket.send(ipc_request.rlp_bytes(), 0).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(2));
    let result_rmp = socket.recv_multipart(DONTWAIT);
    if let Ok(mut rmp) = result_rmp {
        println!("Client received from server, Received multiparts: {:?}", rmp);
        let foo : IpcReply = rlp::decode(&rmp.pop().unwrap()).unwrap();
        println!("Client received from server, IpcReply decoded: {:?}", foo);
        let bar : String = rlp::decode(&foo.result).unwrap();
        println!("Client received from server,  Result decoded: {:?}", bar);
    } else {
        println!("Error: Reply Timeout");
    }

}


