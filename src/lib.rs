//! A steamy river of uring. Fast IO with a API that doesn't make me eyes bleed. GPL-666'd.
//!
//! # Examples
//!
//! # Really shines with O_DIRECT:
//! ```
//! use std::{
//!     fs::OpenOptions,
//!     io::{IoSlice, IoSliceMut, Result},
//!     os::unix::fs::OpenOptionsExt,
//! };
//!
//! const CHUNK_SIZE: u64 = 4096 * 256;
//!
//! // `O_DIRECT` requires all reads and writes
//! // to be aligned to the block device's block
//! // size. 4096 might not be the best, or even
//! // a valid one, for yours!
//! #[repr(align(4096))]
//! struct Aligned([u8; CHUNK_SIZE as usize]);
//!
//! fn main() -> Result<()> {
//!     // start the ring
//!     let mut ring = rio::new().expect("create uring");
//!
//!     // open output file, with `O_DIRECT` set
//!     let file = OpenOptions::new()
//!         .read(true)
//!         .write(true)
//!         .create(true)
//!         .truncate(true)
//!         .custom_flags(libc::O_DIRECT)
//!         .open("file")
//!         .expect("open file");
//!
//!     // create output buffer
//!     let out_buf = Aligned([42; CHUNK_SIZE as usize]);
//!     let out_io_slice = IoSlice::new(&out_buf.0);
//!
//!     // create input buffer
//!     let mut in_buf = Aligned([0; CHUNK_SIZE as usize]);
//!     let mut in_io_slice = IoSliceMut::new(&mut in_buf.0);
//!
//!     let mut completions = vec![];
//!
//!     for i in 0..(4 * 1024) {
//!         let at = i * CHUNK_SIZE;
//!
//!         // Write using `Ordering::Link`,
//!         // causing the next operation to wait
//!         // for the this operation
//!         // to complete before starting.
//!         //
//!         // If this operation does not
//!         // fully complete, the next linked
//!         // operation fails with `ECANCELED`.
//!         //
//!         // io_uring executes unchained
//!         // operations out-of-order to
//!         // improve performance. It interleaves
//!         // operations from different chains
//!         // to improve performance.
//!         let completion = ring.write_ordered(
//!             &file,
//!             &out_io_slice,
//!             at,
//!             rio::Ordering::Link,
//!         )?;
//!         completions.push(completion);
//!
//!         let completion =
//!             ring.read(&file, &mut in_io_slice, at)?;
//!         completions.push(completion);
//!     }
//!
//!     ring.submit_all()?;
//!
//!     let mut canceled = 0;
//!     for completion in completions.into_iter() {
//!         match completion.wait() {
//!             Err(e) if e.raw_os_error() == Some(125) => {
//!                 canceled += 1
//!             }
//!             Ok(_) => {}
//!             other => panic!("error: {:?}", other),
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/spacejam/sled/master/art/tree_face_anti-transphobia.png"
)]
#![cfg_attr(test, deny(warnings))]
#![deny(
    missing_docs,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unused_qualifications
)]
#![deny(
    // over time, consider enabling the following commented-out lints:
    // clippy::missing_const_for_fn,
    // clippy::missing_docs_in_private_items,
    // clippy::module_name_repetitions,
    // clippy::multiple_crate_versions,
    // clippy::unimplemented,
    // clippy::wildcard_enum_match_arm,
    // clippy::else_if_without_else,
    // clippy::float_arithmetic,
    // clippy::indexing_slicing,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::checked_conversions,
    clippy::decimal_literal_representation,
    clippy::doc_markdown,
    clippy::empty_enum,
    clippy::expl_impl_clone_on_copy,
    clippy::explicit_into_iter_loop,
    clippy::explicit_iter_loop,
    clippy::fallible_impl_from,
    clippy::filter_map,
    clippy::filter_map_next,
    clippy::find_map,
    clippy::get_unwrap,
    clippy::if_not_else,
    clippy::inline_always,
    clippy::invalid_upcast_comparisons,
    clippy::items_after_statements,
    clippy::map_flatten,
    clippy::match_same_arms,
    clippy::maybe_infinite_iter,
    clippy::mem_forget,
    clippy::multiple_inherent_impl,
    clippy::mut_mut,
    clippy::needless_borrow,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::non_ascii_literal,
    clippy::option_map_unwrap_or,
    clippy::option_map_unwrap_or_else,
    clippy::path_buf_push_overwrite,
    clippy::print_stdout,
    clippy::pub_enum_variant_names,
    clippy::redundant_closure_for_method_calls,
    clippy::replace_consts,
    clippy::result_map_unwrap_or_else,
    clippy::shadow_reuse,
    clippy::shadow_same,
    clippy::shadow_unrelated,
    clippy::single_match_else,
    clippy::string_add,
    clippy::string_add_assign,
    clippy::type_repetition_in_bounds,
    clippy::unicode_not_nfc,
    clippy::unseparated_literal_suffix,
    clippy::used_underscore_binding,
    clippy::wildcard_dependencies,
    clippy::wrong_pub_self_convention,
)]

use std::io;

mod completion;

#[cfg(target_os = "linux")]
mod io_uring;

#[cfg(target_os = "linux")]
pub use io_uring::{Config, Ordering, Uring as Rio};

pub use completion::Completion;

use completion::{pair, CompletionFiller};

/// Create a new IO system.
pub fn new() -> io::Result<Rio> {
    Config::default().start()
}

#[cfg(test)]
mod use_cases {
    use super::new;

    #[test]
    #[ignore]
    fn broadcast() {
        todo!()
    }

    #[test]
    fn cp() {
        todo!()
    }

    #[test]
    fn logger() {
        todo!()
    }

    #[test]
    fn sled_like() {
        todo!()
    }
}
