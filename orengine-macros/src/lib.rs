extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

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
                    ret = io_request_data_ret;
                    return Poll::Ready(Ok(#ret_statement));
                }
                Err(err) => {
                    unsafe {
                        return Poll::Ready(Err(err));
                    }
                }
            }
        }

        let task = crate::get_task_from_context!(cx);
        this.io_request_data = Some(IoRequestData::new(task));

        #do_request;

        return Poll::Pending;
    };

    TokenStream::from(expanded)
}

/// Generates code for [`Future::poll`](std::future::Future::poll).
///
/// # The difference between `poll_for_io_request` and `poll_for_time_bounded_io_request`
///
/// `poll_for_time_bounded_io_request` deregisters the time bounded task after execution.
///
/// # Must have above
///
/// * `this` with `io_request_data` (`Option<IoRequestData>`) and `deadline`
/// ([`Instant`](std::time::Instant)) fields;
///
/// * `cx` with `waker` method that returns ([`Waker`](std::task::Waker)) which contains
/// `*const crate::runtime::Task` in [`data`](std::task::Waker::data);
///
/// * declared variable `ret` (`usize`) which can be used in `ret_statement`;
///
/// * `worker` from `local_worker()`.
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
                    ret = io_request_data_ret;
                    worker.deregister_time_bounded_io_task(&this.deadline);

                    return Poll::Ready(Ok(#ret_statement));
                }
                Err(err) => {
                    if err.kind() != std::io::ErrorKind::TimedOut {
                        worker.deregister_time_bounded_io_task(&this.deadline);
                    }

                    return Poll::Ready(Err(err));
                }
            }
        }

        let task = crate::get_task_from_context!(cx);
        this.io_request_data = Some(IoRequestData::new(task));

        #do_request;

        return Poll::Pending;
    };

    TokenStream::from(expanded)
}

/// Generates a test function with provided locality.
fn generate_test(input: TokenStream, is_local: bool) -> TokenStream {
    let input = parse_macro_input!(input as syn::ItemFn);
    let body = &input.block;
    let attrs = &input.attrs;
    let signature = &input.sig;
    let name = &signature.ident;
    let name_str = name.to_string();
    if signature.inputs.len() > 0 {
        panic!("Test function must have zero arguments!");
    }
    let spawn_fn = if is_local {
        quote! { orengine::test::run_test_and_block_on_local }
    } else {
        quote! { orengine::test::run_test_and_block_on_shared }
    };

    let expanded = quote! {
        #[test]
        #(#attrs)*
        fn #name() {
            println!("Test {} started!", #name_str.to_string());
            #spawn_fn(async {
                #body
            });
            println!("Test {} finished!", #name_str.to_string());
        }
    };

    TokenStream::from(expanded)
}

/// Generates a test function with running an `Executor` with `local` task.
///
/// # The difference between `test_local` and [`test_shared`]
///
/// `test_local` generates a test function that runs an `Executor` with `local` task.
/// [`test_shared`] generates a test function that runs an `Executor` with `shared` task.
///
/// # Example
///
/// ```ignore
/// #[orengine_macros::test_local]
/// fn test_sleep() {
///     let start = std::time::Instant::now();
///     orengine::sleep(std::time::Duration::from_secs(1)).await;
///     assert!(start.elapsed() >= std::time::Duration::from_secs(1));
/// }
/// ```
///
/// # Note
///
/// Code above is equal to:
///
/// ```ignore
/// #[test]
/// fn test_sleep() {
///     println!("Test sleep started!");
///     orengine::test::run_test_and_block_on_local(async {
///         let start = std::time::Instant::now();
///         orengine::sleep(std::time::Duration::from_secs(1)).await;
///         assert!(start.elapsed() >= std::time::Duration::from_secs(1));
///     });
///     println!("Test sleep finished!");
/// }
/// ```
#[proc_macro_attribute]
pub fn test_local(_: TokenStream, input: TokenStream) -> TokenStream {
    generate_test(input, true)
}

/// Generates a test function with running an `Executor` with `local` task.
///
/// # The difference between `test_shared` and [`test_local`]
///
/// [`test_shared`] generates a test function that runs an `Executor` with `shared` task.
/// `test_local` generates a test function that runs an `Executor` with `local` task.
///
/// # Example
///
/// ```ignore
/// #[orengine_macros::test_shared]
/// fn test_sleep() {
///     let start = std::time::Instant::now();
///     orengine::sleep(std::time::Duration::from_secs(1)).await;
///     assert!(start.elapsed() >= std::time::Duration::from_secs(1));
/// }
/// ```
///
/// # Note
///
/// Code above is equal to:
///
/// ```ignore
/// #[test]
/// fn test_sleep() {
///     println!("Test sleep started!");
///     orengine::test::run_test_and_block_on_shared(async {
///         let start = std::time::Instant::now();
///         orengine::sleep(std::time::Duration::from_secs(1)).await;
///         assert!(start.elapsed() >= std::time::Duration::from_secs(1));
///     });
///     println!("Test sleep finished!");
/// }
/// ```
#[proc_macro_attribute]
pub fn test_shared(_: TokenStream, input: TokenStream) -> TokenStream {
    generate_test(input, false)
}
