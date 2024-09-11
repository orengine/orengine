extern crate proc_macro;
use proc_macro::TokenStream;
use std::ops::Deref;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, Expr, Lit};

#[proc_macro_attribute]
// TODO takes timeout in params
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
