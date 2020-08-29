
mod config;
use clap::{App, load_yaml, ArgMatches, Arg};
use common_types::ipc::{IpcRequest, IpcReply, query_account_info};
use common_types::transaction::{UnverifiedTransaction};
use zmq::{Socket, Context, DEALER, ROUTER, DONTWAIT};
use std::time::Duration;
use hex_literal::hex;
use rlp::{Decodable, DecoderError, Encodable, Rlp, RlpStream};
use std::str::FromStr; // !!! Necessary for H160::from_str(address).expect("...");
use ethereum_types::{H160, H512, H256, U256};
use serde_derive::{Deserialize, Serialize};
use log::info;
use std::env;
use env_logger;
use config::*;



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

    let query_socket = decoded_config.query_socket.unwrap_or("tcp://127.0.0.1".to_string());
    let txpool_socket = decoded_config.txpool_socket.unwrap_or("tcp://127.0.0.1".to_string());

    let mut accounts = accounts_vec();
    send_random_tx(&mut accounts, query_socket.as_str(), txpool_socket.as_str());
}


fn send_random_tx(accounts: &mut Vec<Account>, query_socket_str: &str, txpool_socket_str: &str) {
    use rand::{thread_rng, Rng};
    let mut rng = thread_rng();
    let mut foo: usize;
    let mut bar: usize;
    let mut zee: usize;
    let context = Context::new();
    let txpool_socket = context.socket(DEALER).unwrap();
    txpool_socket.set_identity( &hex!("1234").to_vec() ).unwrap();
    txpool_socket.connect(txpool_socket_str).unwrap();

    let query_socket = context.socket(DEALER).unwrap();
    query_socket.set_identity( &hex!("1234").to_vec() ).unwrap();
    query_socket.connect(query_socket_str).unwrap();

    let mut count : u128 = 0;
    loop {
        foo = 0;
        bar = 0;
        while (foo == bar) {
            foo = rng.gen_range(1, 10);
            bar = rng.gen_range(1, 10);
        }
        zee = (count % 9 + 1) as usize;
        count += 1;
        println!("\n\n####account[0] => account[{}]; account[{}] => account[{}].", zee, foo, bar);

        let sender = &accounts[0];
        let receiver = &accounts[zee];
        let receiver_address = H160::from_str(receiver.address.as_str()).unwrap();
        let sender_address = H160::from_str(sender.address.as_str()).unwrap();
        let tx = sign_tx(
            &query_socket,
            sender,
            1,
            Some(receiver_address),
            U256::from(10000),
            U256::from(10),
            U256::from(24000),
            hex::decode("336699").unwrap(),

        );
        send_to_txpool(
            &txpool_socket,
            hex::encode(tx).as_str(),
            3
        );

        let sender = &accounts[foo];
        let receiver = &accounts[bar];
        let receiver_address = H160::from_str(receiver.address.as_str()).unwrap();
        let sender_address = H160::from_str(sender.address.as_str()).unwrap();
        let tx = sign_tx(
            &query_socket,
            sender,
            1,
            Some(receiver_address),
            U256::from(100),
            U256::from(10),
            U256::from(24000),
            hex::decode("123456").unwrap(),

        );
        send_to_txpool(
            &txpool_socket,
            hex::encode(tx).as_str(),
            3
        );
    }
}


fn send_to_txpool(socket : &Socket, tx : &str, seconds : u64) {
    let foo : UnverifiedTransaction = rlp::decode(&hex::decode(tx).unwrap()).unwrap();
    println!("{:?}", foo);
    let foobar_vec = vec![foo];
    let foobar_bytes = rlp::encode_list(&foobar_vec);

    let ipc_request = IpcRequest {
        method: "SendToTxPool".to_string(),
        id: 666,
        params: foobar_bytes,
    };
    let recovered_request : IpcRequest = rlp::decode(&ipc_request.rlp_bytes()).unwrap();
    println!("Recovered request: {:x?}", recovered_request);

    socket.send(ipc_request.rlp_bytes(), 0).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(seconds));

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


fn sign_tx(
    chain_socket: &Socket,
    account: &Account,
    chain_id: u32,
    to: Option<H160>,
    value: U256,
    gas_price: U256,
    gas: U256,
    data: Vec<u8>
) -> Vec<u8> {
    let addr = H160::from_str(account.address.as_str()).unwrap();
    let (nonce, balance) = query_account_info(&chain_socket, &addr);
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


