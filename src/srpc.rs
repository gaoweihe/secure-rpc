#[allow(unused_imports)]
use secrpc::core::srpc_core::RpcCore;

use secrpc::{msg::srpc_msg::{RpcMsgHandle, RpcOnceMsg}, core::srpc_core::RPC_DISPATCHER, conf::conf::RpcConf};
use tracing::{info, Level, trace};
use tracing_subscriber::{FmtSubscriber};

#[allow(unused_variables)]
fn simple_callback(msg_handle: RpcMsgHandle)
{
    trace!("simple_callback: msg_handle = {:?}", msg_handle);
    let data = msg_handle.msg.payload.msg_data; 
    let str = String::from_utf8(data).unwrap();
    info!("simple_callback: msg = {:?}", str);
}

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    RpcConf::init_conf();

    // set tracer 
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed"); 

    RpcConf::print_conf();

    let rpc_core = RpcCore::new();
    assert!(RPC_DISPATCHER.get().is_some());
    let _reg_result = 
        rpc_core.reg_legacy_cb(
            1, 
            Box::new(simple_callback)
        );
    let _result = rpc_core.start();

    let conf = RpcConf::get_conf();
    let rmt_grpc_uri = conf.rmt_grpc_uri
        .get(0).unwrap(); 
    let peer_id = 1;

    let _session_id = 
        rpc_core.dispatcher.connect_to(peer_id, rmt_grpc_uri).await;

    // sleep for 1 second(s)
    // for better trace log (optional) 
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let mut req = RpcMsgHandle::default();
    let mut msg = RpcOnceMsg::default();
    msg.req_type = 1;
    let raw_str = "hello"; 
    info!("main: push_req: {:?}", raw_str);
    msg.payload.msg_data = raw_str.as_bytes().to_vec();
    req.set_msg(msg);
    req.peer_id = peer_id;

    loop {
        // push request 
        rpc_core.dispatcher.push_req(req.clone());  

        // run event loop
        rpc_core.dispatcher.run_loop_once();
    }

}
