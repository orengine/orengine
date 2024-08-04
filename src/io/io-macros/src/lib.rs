extern crate proc_macro;
use proc_macro::TokenStream;

use quote::quote;
use syn::parse_macro_input;

#[proc_macro]
pub fn poll_for_io_request(input: TokenStream) -> TokenStream {
    let input_elems = parse_macro_input!(input as syn::ExprTuple).elems;

    let do_request = &input_elems[0];
    let ret_statement = &input_elems[1];

    let expanded = quote! {
        if this.io_request.is_none() {
            unsafe {
                let task = (cx.waker().as_raw().data() as *const Task).read();
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
                let task = (cx.waker().as_raw().data() as *const Task).read();
                this.io_request = Some(IoRequest::new(task));

                unsafe {
                    this.time_bounded_io_task.set_user_data(this.io_request.as_ref().unwrap_unchecked() as *const _ as u64);
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
