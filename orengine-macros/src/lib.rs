extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use std::ops::Deref;
use syn::{parse_macro_input, Expr, Lit};

/// Returns the `core::time::Duration::from_secs(#timeout_secs)` expression.
///
/// It expects an expression like `timeout_secs = "1"`
///
/// # Example
///
/// ```ignore
/// use orengine_macros::get_timeout_name;
///
/// let timeout = get_timeout_name(quote! { timeout_secs = "123" }); // core::time::Duration::from_secs(123)
/// ```
///
fn get_timeout_name(attr: TokenStream) -> proc_macro2::TokenStream {
    let mut timeout = quote! { core::time::Duration::from_secs(1) };
    match syn::parse::<syn::ExprAssign>(attr) {
        Ok(syntax_tree) => {
            if syntax_tree.left.into_token_stream().to_string() == "timeout_secs" {
                match syntax_tree.right.deref() {
                    Expr::Lit(lit) => {
                        if let Lit::Str(expr) = lit.lit.clone() {
                            let token_stream: u64 = expr.value().parse().unwrap();
                            timeout = quote! { core::time::Duration::from_secs(#token_stream) };
                        }
                    }

                    _ => {}
                }
            }
        }
        Err(_err) => {}
    }

    timeout
}

/// Generates a test function with a [`timeout`](get_timeout_name) argument.
fn generate_test(args: TokenStream, input: TokenStream, is_local: bool) -> TokenStream {
    let timeout = get_timeout_name(args);
    let input = parse_macro_input!(input as syn::ItemFn);
    let body = &input.block;
    let attrs = &input.attrs;
    let signature = &input.sig;
    let name = signature.ident.to_string();
    let spawn_fn = if is_local {
        quote! { run_with_local_future }
    } else {
        quote! { run_with_global_future }
    };

    let expanded = quote! {
        #[test]
        #(#attrs)*
        #signature {
            let lock = crate::utils::global_test_lock::GLOBAL_TEST_LOCK.lock();
            let (sender, receiver) = std::sync::mpsc::channel();
            println!("test {} is started!", #name);

            let res = std::thread::spawn(move || {
                let sender = std::sync::Arc::new(sender);
                let sender2 = sender.clone();
                let result = std::panic::catch_unwind(move || {
                    let executor = crate::Executor::init();
                    let _ = executor.#spawn_fn(async move {
                        #body
                        sender2.send(Ok(())).unwrap();
                    });
                });

                if let Err(err) = result {
                    sender.send(Err(err)).unwrap();
                }
            });

            let res = receiver.recv_timeout(#timeout);
            unsafe { crate::runtime::stop_all_executors() };
            match res {
                Ok(Ok(())) => {
                    println!("test {} is finished!", #name);
                    println!();
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    drop(lock);
                }
                Ok(Err(err)) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    drop(lock);
                    std::panic::resume_unwind(err);
                },
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    drop(lock);
                    panic!("test {} is failed (timeout)!", #name)
                },
            }
        }
    };

    TokenStream::from(expanded)
}

/// Generates a test function with running an `Executor` with `local` task.
/// It also prevents running tests in parallel.
///
/// # The difference between `test` and `test_global`
///
/// `test` generates a test function that runs an `Executor` with `local` task.
/// `test_global` generates a test function that runs an `Executor` with `global` task.
///
/// # Optional arguments
///
/// * timeout_secs - the timeout in seconds
///
/// # Example
///
/// ```ignore
/// #[orengine_macros::test(timeout_secs = "2")]
/// fn test_sleep() {
///     let start = std::time::Instant::now();
///     orengine::sleep(std::time::Duration::from_secs(1)).await;
///     assert!(start.elapsed() >= std::time::Duration::from_secs(1));
/// }
/// ```
#[proc_macro_attribute]
pub fn test(args: TokenStream, input: TokenStream) -> TokenStream {
    generate_test(args, input, true)
}

/// Generates a test function with running an `Executor` with `global` task.
/// It also prevents running tests in parallel.
///
/// # The difference between `test_global` and `test`
///
/// `test_global` generates a test function that runs an `Executor` with `global` task.
/// `test` generates a test function that runs an `Executor` with `local` task.
///
/// # Optional arguments
///
/// * timeout_secs - the timeout in seconds
///
/// # Example
///
/// ```ignore
/// #[orengine_macros::test_global(timeout_secs = "2")]
/// fn test_sleep() {
///     let start = std::time::Instant::now();
///     orengine::sleep(std::time::Duration::from_secs(1)).await;
///     assert!(start.elapsed() >= std::time::Duration::from_secs(1));
/// }
/// ```
#[proc_macro_attribute]
pub fn test_global(args: TokenStream, input: TokenStream) -> TokenStream {
    generate_test(args, input, false)
}

/// Generates code for [`Future::poll`](std::future::Future::poll).
///
/// # Must have above
///
/// * `this` with `io_request_data` (`Option<IoRequestData>`) fields;
///
/// * `cx` with `waker` method that returns ([`Waker`](std::task::Waker)) which contains
/// `*const crate::runtime::Task` in [`data`](std::task::Waker::data);
///
/// * declared variable `ret` (`usize`) which can be used in `ret_statement`.
///
/// # Arguments
///
/// * `do_request` - the request code
/// * `ret_statement` - the return statement which can use `ret`(`usize`) variable.
#[proc_macro]
pub fn poll_for_io_request(input: TokenStream) -> TokenStream {
    let input_elems = parse_macro_input!(input as syn::ExprTuple).elems;

    let do_request = &input_elems[0];
    let ret_statement = &input_elems[1];

    let expanded = quote! {
        if let Some(mut io_request_data) = this.io_request_data.take() {
            match io_request_data.ret() {
                Ok(io_request_data_ret) => {
                    unsafe {
                        ret = io_request_data_ret;
                        return Poll::Ready(Ok(#ret_statement));
                    }
                }
                Err(err) => {
                    unsafe {
                        return Poll::Ready(Err(err));
                    }
                }
            }
        }

        unsafe {
            let task = (cx.waker().data() as *const crate::runtime::Task).read();
            this.io_request_data = Some(IoRequestData::new(task));

            #do_request;

            return Poll::Pending;
        }
    };

    TokenStream::from(expanded)
}

/// Generates code for [`Future::poll`](std::future::Future::poll).
///
/// # Must have above
///
/// * `this` with `io_request_data` (`Option<IoRequestData>`) and `deadline`
/// ([`Instant`](std::time::Instant)) fields;
///
/// * `cx` with `waker` method that returns ([`Waker`](std::task::Waker)) which contains
/// `*const crate::runtime::Task` in [`data`](std::task::Waker::data);
///
/// * declared variable `ret` (`usize`) which can be used in `ret_statement`.
///
/// # Arguments
///
/// * `do_request` - the request code
/// * `ret_statement` - the return statement which can use `ret`(`usize`) variable.
#[proc_macro]
pub fn poll_for_time_bounded_io_request(input: TokenStream) -> TokenStream {
    let input_elems = parse_macro_input!(input as syn::ExprTuple).elems;

    let do_request = &input_elems[0];
    let ret_statement = &input_elems[1];

    let expanded = quote! {
        if let Some(mut io_request_data) = this.io_request_data.take() {
            match io_request_data.ret() {
                Ok(io_request_data_ret) => {
                    unsafe {
                        ret = io_request_data_ret;
                        worker.deregister_time_bounded_io_task(&this.deadline);
                        return Poll::Ready(Ok(#ret_statement));
                    }
                }
                Err(err) => {
                    if err.kind() != std::io::ErrorKind::TimedOut {
                        worker.deregister_time_bounded_io_task(&this.deadline);
                    }

                    return Poll::Ready(Err(err));
                }
            }
        }

        unsafe {
            let task = (cx.waker().data() as *const crate::runtime::Task).read();
            this.io_request_data = Some(IoRequestData::new(task));

            worker.register_time_bounded_io_task(
                unsafe { this.io_request_data.as_ref().unwrap_unchecked() },
                &mut this.deadline,
            );
            #do_request;

            return Poll::Pending;
        }
    };

    TokenStream::from(expanded)
}
