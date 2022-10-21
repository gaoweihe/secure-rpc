use tracing::{info};
use argparse::{ArgumentParser, Store, StoreTrue, List};

use once_cell::sync::OnceCell;
pub static RPC_CONF: once_cell::sync::OnceCell<RpcConf> = 
    OnceCell::new();

#[derive(Debug, Clone)]
pub struct RpcConf {
    pub rmt_grpc_uri: Vec<String>, 
    pub loc_mr_size: u32, 
}

impl RpcConf {
    fn new_singleton() -> Self {
        let mut rmt_grpc_uri = Vec::new();
        let loc_mr_size = 0;
        let mut conf = Self {
            rmt_grpc_uri, 
            loc_mr_size,
        };
        conf
    }

    pub fn init_conf() {
        let conf = Self::new_singleton();
        Self::parse_args();
    }

    fn parse_args() {
        let mut conf = RpcConf::new_singleton();

        {
            let mut ap = ArgumentParser::new(); 
            ap.set_description("Secure RPC service. ");

            ap.refer(&mut conf.rmt_grpc_uri)
                .add_option(
                    &["--peer"], 
                    List, 
                    "Remote gRPC URI. "
                );
            ap.refer(&mut conf.loc_mr_size)
                .add_option(
                    &["--mrsize"], 
                    Store, 
                    "Local memory region size. "
                );
            
            ap.parse_args_or_exit(); 
        }

        RPC_CONF.get_or_init(|| {
            conf
        });
    }

    pub fn get_conf() -> &'static Self {
        RPC_CONF.get().unwrap()
    }

    pub fn print_conf() {
        let conf = Self::get_conf();
        info!("{:?}", conf);
    }
}