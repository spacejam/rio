//! A steamy river of uring. Fast IO with a API that doesn't make me eyes bleed. GPL-666'd.
//!
//! io_uring is going to change everything. It will speed up your
//! disk usage by like 300%. Go ahead, run the `O_DIRECT` example
//! and compare that to using a threadpool or any other async shit
//! you want. It's not gonna come close!
//!
//! Starting in linux 5.5, it also has support for tcp accept.
//! This is gonna fucking shred everything out there!!!
//!
//! But there's a few snags. Mainly, it's a little misuse-prone.
//! But Rust is pretty nice for specifying proofs about
//! memory usage in the type system. And we don't even have
//! to get too squirley. Check out the `write_at` implementation,
//! for example. It just says that the Completion, the underlying
//! uring, the file being used, the buffer being used, etc...
//! will all be in scope at the same time while the Completion
//! is in-use.
//!
//! This library aims to be misuse-resistant.
//! Most of the other io_uring libraries make
//! it really easy to blow ur ass off with
//! use-after-frees. `rio` uses a ton of
//! lifetime magic to make this stuff fail
//! to compile. Also, if a `Completion`
//! that was pinned to the lifetime of a uring
//! and backing buffer is dropped, it
//! waits for its backing operation to complete
//! before returning from Drop, to further
//! prevent use-after-frees. use-after-frees
//! can SUCK MY ASS!!!
//!
//! # Examples
//!
//! This shit won't compile:
//!
//! ```compile_fail
//! let rio = rio::new().unwrap();
//! let file = std::fs::File::open("fuck_you_use_after_free_you_suck").unwrap();
//! let out_buf = vec![42; 666];
//! let out_io_slice = std::io::IoSlice::new(&out_buf);
//!
//! let completion = rio.write_at(&file, &out_io_slice, 0).unwrap();
//!
//! // At this very moment, the kernel has a pointer to that there slice.
//! // It also has the raw file descriptor of the file.
//! // It's fixin' to write the data from that memory into the file.
//! // But if we freed the shit, it would get megafucked,
//! // and the kernel would write potentially scandalous data
//! // into the file instead.
//!
//! // any of the next 3 lines would cause compilation to fail...
//! drop(out_io_slice);
//! drop(file);
//! drop(rio);
//!
//! // this is both a Future and a normal blocking promise thing.
//! // If you're using async, just call `.await` on it instead
//! // of `.wait()`
//! completion.wait();
//!
//! // now it's safe to drop that shit, whatever...
//! ```
//!
//!
//! Really shines with O_DIRECT:
//!
//! ```no_run
//! use std::{
//!     fs::OpenOptions,
//!     io::{IoSlice, Result},
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
//!     let ring = rio::new().expect("create uring");
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
//!     let mut completions = vec![];
//!
//!     for i in 0..(4 * 1024) {
//!         let at = i * CHUNK_SIZE;
//!
//!         let completion = ring.write_at(
//!             &file,
//!             &out_io_slice,
//!             at,
//!         )?;
//!         completions.push(completion);
//!     }
//!
//!     ring.submit_all()?;
//!
//!     for completion in completions.into_iter() {
//!         completion.wait()?;
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
    // clippy::missing_docs_in_private_items,
    // clippy::else_if_without_else,
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
    clippy::explicit_into_iter_loop,
    clippy::explicit_iter_loop,
    clippy::expl_impl_clone_on_copy,
    clippy::fallible_impl_from,
    clippy::filter_map,
    clippy::filter_map_next,
    clippy::find_map,
    clippy::float_arithmetic,
    clippy::get_unwrap,
    clippy::if_not_else,
    clippy::inline_always,
    clippy::invalid_upcast_comparisons,
    clippy::items_after_statements,
    clippy::map_flatten,
    clippy::match_same_arms,
    clippy::maybe_infinite_iter,
    clippy::mem_forget,
    clippy::missing_const_for_fn,
    clippy::module_name_repetitions,
    clippy::multiple_crate_versions,
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
    clippy::unimplemented,
    clippy::unseparated_literal_suffix,
    clippy::used_underscore_binding,
    clippy::wildcard_dependencies,
    clippy::wildcard_enum_match_arm,
    clippy::wrong_pub_self_convention,
)]

use std::io;

mod completion;
mod histogram;
mod lazy;
mod metrics;

#[cfg(target_os = "linux")]
mod io_uring;

#[cfg(target_os = "linux")]
pub use io_uring::{Config, Ordering, Uring as Rio};

pub use completion::Completion;

use {
    completion::{pair, Filler},
    histogram::Histogram,
    lazy::Lazy,
    metrics::{Measure, M},
};

/// Create a new IO system.
pub fn new() -> io::Result<Rio> {
    Config::default().start()
}

/// Encompasses various types of IO structures that
/// can be operated on as if they were a libc::iovec
pub trait AsIoVec {
    /// Returns the address of this object.
    fn into_new_iovec(&self) -> libc::iovec {
        let ptr: *const _ = self;
        let iovec_ptr = ptr as *const libc::iovec;

        #[allow(unsafe_code)]
        unsafe {
            *iovec_ptr
        }
    }
}

impl AsIoVec for libc::iovec {}

impl<'a> AsIoVec for std::io::IoSlice<'a> {}

impl<'a> AsIoVec for std::io::IoSliceMut<'a> {}

impl<'a> AsIoVec for &'a [u8] {
    fn into_new_iovec(&self) -> libc::iovec {
        libc::iovec {
            iov_base: self.as_ptr() as *mut _,
            iov_len: self.len(),
        }
    }
}

#[cfg(test)]
mod use_cases {
    #[test]
    #[ignore]
    fn broadcast() {
        todo!()
    }

    #[test]
    #[ignore]
    fn cp() {
        todo!()
    }

    #[test]
    #[ignore]
    fn logger() {
        todo!()
    }

    #[test]
    #[ignore]
    fn sled_like() {
        todo!()
    }
}
