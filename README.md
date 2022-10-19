# Secure RPC 
Secure RPC service. 

## Run the demo 
RPC demo: `cargo run --bin srpc` 

IBverbs test: `cargo run --bin ibverbs` 

Run the loopback example: `cargo run --bin srpc-example` 

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


