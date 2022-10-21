SRPC_HOME=/home/gaowh/onlive/secrpc

cargo run --bin srpc \
    -- \
    --peer http://127.0.0.1:9000 \
    --mrsize 256 \
    >srpc.log 2>&1 

