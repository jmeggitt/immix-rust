use std::env;
use std::mem::size_of;

mod exhaust;
mod gcbench;
// mod mark;
mod mt_gcbench;
mod mt_trace;
mod obj_init;
mod trace;

fn main() {
    let mut immix_space_size = 1usize << 30;

    match env::var("HEAP_SIZE") {
        Ok(val) => {
            if val.ends_with('M') {
                let (num, _) = val.split_at(val.len() - 1);
                immix_space_size = num.parse::<usize>().unwrap() << 20;

                println!(
                    "heap is {} bytes (immix: {} bytes) . ",
                    immix_space_size, immix_space_size
                );
            } else {
                println!("unknown heap size variable: {}, ignore", val);
                println!("using default heap size: {} bytes. ", immix_space_size);
            }
        }
        Err(_) => {
            println!(
                "using default heap size: {} bytes ({} MB). ",
                immix_space_size,
                immix_space_size >> 20
            );
        }
    }

    println!("The current machine has {} cpus!", num_cpus::get());
    println!("Program compiled in {}x mode!", 8 * size_of::<*mut ()>());

    if cfg!(feature = "exhaust") {
        exhaust::exhaust_alloc(immix_space_size);
    } else if cfg!(feature = "initobj") {
        obj_init::alloc_init(immix_space_size);
    } else if cfg!(feature = "gcbench") {
        gcbench::start(immix_space_size);
    } else if cfg!(feature = "mt-gcbench") {
        mt_gcbench::start(immix_space_size);
    } else if cfg!(feature = "mark") {
        // mark::alloc_mark();
    } else if cfg!(feature = "trace") {
        trace::alloc_trace(immix_space_size);
    } else if cfg!(feature = "mt-trace") {
        mt_trace::alloc_trace(immix_space_size);
    } else {
        println!("unknown features: build with 'cargo build --release --features \"exhaust\"");
    }
}
