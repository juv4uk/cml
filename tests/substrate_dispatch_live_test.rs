use cml::gpu_worker_client::gpu_worker_socket_path;
use cml::ir::{BufferLiteral, Ir, Params};
use cml::placement::{PlacementConfig, PlacementOverride};
use cml::substrate_dispatch::SubstrateDispatcher;

fn add_map(values: Vec<i32>, offset: i64) -> Ir {
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["X".into()]),
                body: Box::new(Ir::App {
                    func: Box::new(Ir::Sid(sens::sens!(00001100))),
                    args: vec![Ir::Var("X".into()), Ir::Int(offset)],
                }),
            },
            Ir::Buffer(BufferLiteral::I32(values)),
        ],
    }
}

#[test]
fn live_shared_cuda_worker_executes_admitted_map() {
    if std::env::var_os("CML_SHARED_WORKER_LIVE").is_none() {
        eprintln!("SKIP: set CML_SHARED_WORKER_LIVE=1 for the owner-hardware witness");
        return;
    }

    let socket = gpu_worker_socket_path();
    assert!(
        socket.exists(),
        "shared worker socket is missing: {}",
        socket.display()
    );

    let backend = "shared-cuda";
    let mut dispatcher = SubstrateDispatcher::new(PlacementConfig::default(), 4);
    dispatcher.register_shared_cuda_worker(backend, &socket, true);

    let outcome = dispatcher
        .execute_numeric_map(
            &add_map(vec![1, 2, 3, 4], 7),
            Some(&PlacementOverride::Gpu {
                backend_tag: backend.into(),
            }),
        )
        .unwrap();

    assert_eq!(
        outcome.value,
        BufferLiteral::I32(vec![8, 9, 10, 11])
    );
}
