/*
 * This example just does 1 million NOPs,
 * stressing the rio library and io_uring,
 * without triggering any device IO.
 */

use std::io::Result;

fn main() -> Result<()> {
    // start the ring
    let mut config = rio::Config::default();
    config.print_profile_on_drop = true;
    let ring = config.start().expect("create uring");

    let mut completions = vec![];

    let pre = std::time::Instant::now();

    for _ in 0..(1024 * 1024) {
        let completion = ring.nop()?;
        completions.push(completion);
    }

    let post_submit = std::time::Instant::now();

    ring.submit_all()?;

    for completion in completions.into_iter() {
        completion.wait().unwrap();
    }

    let post_complete = std::time::Instant::now();

    dbg!(post_submit - pre, post_complete - post_submit);

    Ok(())
}
