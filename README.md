# Secure RPC 
Secure RPC service. 

## Run the demo 
RPC demo: `cargo run --bin srpc -- --peer http://127.0.0.1:9000` 

IBverbs test: `cargo run --bin ibverbs` 

## Source files

1. ` src/core/ ` 

`srpc_core.rs `
RPC service kernel routine. 

`srpc_dispatcher.rs `
Features RPC requests/ responses dispatching.   

`srpc_session.rs `
Context of RPC connection to the remote ends. 

2. ` src/msg/ `

`srpc_msg.rs `
RPC message formats and ser/de. 

3. ` src/tee/ `

`srpc_tee_sgx.rs`
Enclave functionalities, to be developed. 


