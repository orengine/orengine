extern crate proc_macro;

use proc_macro::TokenStream;
use std::ops::Deref;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, Expr, Lit};

#[proc_macro_attribute]
pub fn test(args: TokenStream, input: TokenStream) -> TokenStream {
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
            },
            Err(_err) => {},
        }


        timeout
    }

    let timeout = get_timeout_name(args);
    let input = parse_macro_input!(input as syn::ItemFn);
    let body = &input.block;
    let attrs = &input.attrs;
    let signature = &input.sig;
    let name = signature.ident.to_string();

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
                    let _ = executor.run_and_block_on(async move {
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

#[proc_macro]
pub fn poll_for_io_request(input: TokenStream) -> TokenStream {
    let input_elems = parse_macro_input!(input as syn::ExprTuple).elems;

    let do_request = &input_elems[0];
    let ret_statement = &input_elems[1];

    let expanded = quote! {
        if this.io_request.is_none() {
            unsafe {
                let task = (cx.waker().as_raw().data() as *const crate::runtime::Task).read();
                this.io_request = Some(IoRequest::new(task));

                #do_request;

                return Poll::Pending;
            }
        }

        let ret_ = unsafe { this.io_request.take().unwrap_unchecked() }.ret();

        if ret_.is_ok() {
            unsafe {
                ret = ret_.unwrap_unchecked();
                return Poll::Ready(Ok(#ret_statement));
            }
        }

        return Poll::Ready(Err(unsafe { ret_.unwrap_err_unchecked() }));
    };

    TokenStream::from(expanded)
}

#[proc_macro]
pub fn poll_for_time_bounded_io_request(input: TokenStream) -> TokenStream {
    let input_elems = parse_macro_input!(input as syn::ExprTuple).elems;

    let do_request = &input_elems[0];
    let ret_statement = &input_elems[1];

    let expanded = quote! {
        if this.io_request.is_none() {
            unsafe {
                let task = (cx.waker().as_raw().data() as *const crate::runtime::Task).read();
                this.io_request = Some(IoRequest::new(task));

                unsafe {
                    this.time_bounded_io_task.set_user_data(this.io_request.as_mut().unwrap_unchecked() as *const _ as u64);
                }

                worker.register_time_bounded_io_task(&mut this.time_bounded_io_task);
                #do_request;

                return Poll::Pending;
            }
        }

        let ret_ = unsafe { this.io_request.take().unwrap_unchecked() }.ret();
        if ret_.is_ok() {
            unsafe {
                ret = ret_.unwrap_unchecked();
                worker.deregister_time_bounded_io_task(&this.time_bounded_io_task);
                return Poll::Ready(Ok(#ret_statement));
            }
        }

        let err = unsafe { ret_.unwrap_err_unchecked() };
        if err.kind() != std::io::ErrorKind::TimedOut {
            worker.deregister_time_bounded_io_task(&this.time_bounded_io_task);
        }

        return Poll::Ready(Err(err));
    };

    TokenStream::from(expanded)
}