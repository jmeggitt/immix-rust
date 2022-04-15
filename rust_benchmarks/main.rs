use immix_rust::{heap, objectmodel};
use std::env;
use std::mem::size_of;
use std::sync::atomic::Ordering;

mod exhaust;
mod gcbench;
// mod mark;
mod mt_gcbench;
mod mt_trace;
mod obj_init;
mod trace;

fn init() {
    objectmodel::init();
}

fn main() {
    init();

    match env::var("HEAP_SIZE") {
        Ok(val) => {
            if val.ends_with('M') {
                let (num, _) = val.split_at(val.len() - 1);
                let heap_size = num.parse::<usize>().unwrap() << 20;

                let immix_space_size: usize = (heap_size as f64 * heap::IMMIX_SPACE_RATIO) as usize;
                heap::IMMIX_SPACE_SIZE.store(immix_space_size, Ordering::SeqCst);

                println!(
                    "heap is {} bytes (immix: {} bytes) . ",
                    heap_size, immix_space_size
                );
            } else {
                println!("unknow heap size variable: {}, ignore", val);
                println!(
                    "using default heap size: {} bytes. ",
                    heap::IMMIX_SPACE_SIZE.load(Ordering::SeqCst)
                );
            }
        }
        Err(_) => {
            let heap_size = heap::IMMIX_SPACE_SIZE.load(Ordering::SeqCst);
            println!(
                "using default heap size: {} bytes ({} MB). ",
                heap_size,
                heap_size >> 20
            );
        }
    }

    println!("The current machine has {} cpus!", num_cpus::get());
    println!("Program compiled in {}x mode!", 8 * size_of::<*mut ()>());

    if cfg!(feature = "exhaust") {
        exhaust::exhaust_alloc();
    } else if cfg!(feature = "initobj") {
        obj_init::alloc_init();
    } else if cfg!(feature = "gcbench") {
        gcbench::start();
    } else if cfg!(feature = "mt-gcbench") {
        mt_gcbench::start();
    } else if cfg!(feature = "mark") {
        // mark::alloc_mark();
    } else if cfg!(feature = "trace") {
        trace::alloc_trace();
    } else if cfg!(feature = "mt-trace") {
        mt_trace::alloc_trace();
    } else {
        println!("unknown features: build with 'cargo build --release --features \"exhaust\"");
    }
}
