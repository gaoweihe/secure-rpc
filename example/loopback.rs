#[allow(unused_imports)]
use tracing::info;

static CONTEXT: once_cell::sync::OnceCell<ibverbs::Context> = 
    once_cell::sync::OnceCell::new(); 
static PROTECTION_DOMAIN: once_cell::sync::OnceCell<ibverbs::ProtectionDomain> = 
    once_cell::sync::OnceCell::new(); 
static COMPLETION_QUEUE: once_cell::sync::OnceCell<ibverbs::CompletionQueue> =
    once_cell::sync::OnceCell::new();

#[allow(unused_variables)]
#[tokio::main]
async fn main() {
    // set tracer 
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed"); 

    CONTEXT.set(
        ibverbs::devices()
            .unwrap()
            .iter()
            .next()
            .expect("no rdma device available")
            .open()
            .unwrap()
    ).unwrap(); 
    let ctx = CONTEXT.get().unwrap();

    PROTECTION_DOMAIN.set(
        ctx.alloc_pd().unwrap()
    ).unwrap();
    let pd = PROTECTION_DOMAIN.get().unwrap();

    COMPLETION_QUEUE.set(
        ctx.create_cq(32, 0).unwrap()
    ).unwrap();
    let cq = COMPLETION_QUEUE.get().unwrap();

    let wrid_cnt = std::sync::atomic::AtomicU64::new(1);

    let qp_builder = pd.create_qp(
        &cq, 
        8, 
        &cq, 
        8, 
        ibverbs::ibv_qp_type::IBV_QPT_RC
    ).build().unwrap();

    let endpoint = qp_builder.endpoint();
    let mut qp = qp_builder.handshake(endpoint).unwrap();

    let mut wr_id = 0;
    let mut req_cnt = 0;

    let pd_post = pd.clone();
    let cq_post = cq.clone(); 
    let post_handle = tokio::spawn(async move {
        let mut mr1 = pd_post.allocate::<u64>(409600).unwrap();
        let mut mr2 = pd_post.allocate::<u64>(409600).unwrap();
        mr1[1] = 0x42; 

        // get current time 
        let start = std::time::Instant::now(); 

        while req_cnt < 2000 {
            // let wr_id = wrid_cnt.fetch_add(
            //     1, 
            //     std::sync::atomic::Ordering::SeqCst
            // );
            let result = unsafe { qp.post_receive(&mut mr2, .., wr_id) };
            match result {
                Ok(_) => {
                    // info!("post_receive: OK: wr_id = {}", wr_id);
                    wr_id += 1;
                    req_cnt += 1;
                },
                Err(e) => {
                    // info!("post_receive: Err: {:?}", e);
                }
            }

            // let wr_id = wrid_cnt.fetch_add(
            //     1, 
            //     std::sync::atomic::Ordering::SeqCst
            // );
            // info!("post_send: wr_id = {}", wr_id);
            let result = unsafe { qp.post_send(&mut mr1, .., wr_id) };
            match result {
                Ok(_) => {
                    wr_id += 1;
                    // info!("post_send: OK: wr_id = {}", wr_id);
                },
                Err(e) => {
                    // info!("post_send: Err: {:?}", e);
                }
            }
        }

        // get current time 
        let end = std::time::Instant::now();

        // calculate the elapsed time
        let elapsed = end - start;

        // print the elapsed time
        info!("Elapsed: {:?} ns", elapsed.as_nanos());

        std::process::exit(exitcode::OK);
    });

    let cq_poll = cq.clone();
    let poll_handle = tokio::spawn(async move {
        let mut completions = [ibverbs::ibv_wc::default(); 32];
        loop {
            let completed = cq_poll.poll(&mut completions[..]).unwrap();
            if completed.is_empty() {
                continue;
            }
            // info!("completed len: {:?}", completed.len());
            for wc in completed {
                match wc.opcode() {
                    ibverbs::ibv_wc_opcode::IBV_WC_SEND => {
                        // info!("sent");
                    }
                    ibverbs::ibv_wc_opcode::IBV_WC_RECV => {
                        // assert_eq!(mr2[0], 0x42);
                        // info!("received");
                    }
                    _ => {
                        panic!("unexpected completion code {:?}", wc.opcode());
                    },
                }
            }
        }
    }); 
    
    post_handle.await.unwrap();
    poll_handle.await.unwrap();
}
