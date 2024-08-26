extern crate proc_macro;
use proc_macro::TokenStream;

use quote::quote;
use syn::parse_macro_input;

#[proc_macro_attribute]
pub fn test(_: TokenStream, input: TokenStream) -> TokenStream {
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
                let result = std::panic::catch_unwind(|| {
                    crate::runtime::create_local_executer_for_block_on(async {
                        #body
                        crate::end::end();
                    });
                });

                if let Err(err) = result {
                    crate::end::end();
                    sender.send(Err(err)).unwrap();
                } else {
                    sender.send(Ok(())).unwrap();
                }
            });

            match receiver.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(Ok(())) => {
                    println!("test {} is finished!", #name);
                    println!();
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    drop(lock);
                }
                Ok(Err(err)) => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    drop(lock);
                    std::panic::resume_unwind(err);
                },
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    drop(lock);
                    panic!("test {} is failed (timeout)!", #name)
                },
            }
        }
    };

    TokenStream::from(expanded)
}