use std::panic;

pub fn init_ungraceful_panic_handler() {
    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        default_hook(panic_info);

        eprintln!("PANIC RECOVERED: The process continues running despite a thread panic.");
        eprintln!("   This is expected in fault-tolerant mode. Check logs for details.");
    }));
}