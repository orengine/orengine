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
            let lock = crate::utils::global_test_lock::GLOBAL_TEST_LOCK.lock(#name.to_string());

            let result = std::panic::catch_unwind(|| {
                crate::runtime::create_local_executer_for_block_on(async {
                    #body
                    crate::end::end();
                });
            });

            if let Err(err) = result {
                crate::end::end();
                std::thread::sleep(std::time::Duration::from_millis(1));
                drop(lock);
                std::panic::resume_unwind(err);
            }

            std::thread::sleep(std::time::Duration::from_millis(1));
            drop(lock);
        }
    };

    TokenStream::from(expanded)
}