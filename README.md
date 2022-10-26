# Secure RPC 
Secure RPC service. 

## Run the perf test  
On server side: 
```
cd /path/to/project/example
cargo run --release --bin loopback-server
``` 

On client side: 
```
cd /path/to/project/example
cargo run --release --bin loopback-client -- --server http://10.0.0.11:50051
```

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


