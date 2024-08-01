extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{quote};
use syn::{parse_macro_input};

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

// Now IDE doesn't support this.
// fn generate_recv_name(methods_name: proc_macro2::TokenStream, postfix: &str) -> proc_macro2::TokenStream {
//     let methods_name = methods_name.to_string();
//     let methods_name = format!("{methods_name}_{}", postfix);
//     proc_macro2::TokenStream::from_str(&methods_name).expect("failed to parse")
// }
//
// fn generate_recv_name_with_prefix(methods_name: proc_macro2::TokenStream, prefix: &str, postfix: &str) -> proc_macro2::TokenStream {
//     let methods_name = methods_name.to_string();
//     let methods_name = format!("{prefix}_{methods_name}_{postfix}");
//     proc_macro2::TokenStream::from_str(&methods_name).expect("failed to parse")
// }
//
// #[proc_macro]
// pub fn generate_recv_methods(input: TokenStream) -> TokenStream {
//     let input_elems = parse_macro_input!(input as syn::ExprTuple).elems;
//     let methods_name = input_elems[0].clone().into_token_stream();
//     let struct_name = input_elems[1].clone().into_token_stream();
//     let struct_name_with_deadline = input_elems[2].clone().into_token_stream();
//
//     let recv_with_deadline = generate_recv_name(methods_name.clone(), "with_deadline");
//     let recv_with_timeout = generate_recv_name(methods_name.clone(), "with_timeout");
//     let poll_recv_buf = generate_recv_name_with_prefix(methods_name.clone(), "poll", "buf");
//     let poll_recv_buf_with_deadline = generate_recv_name_with_prefix(
//         methods_name.clone(),
//         "poll",
//         "buf_with_deadline"
//     );
//     let poll_recv_buf_with_timeout = generate_recv_name_with_prefix(
//         methods_name.clone(),
//         "poll",
//         "buf_with_timeout"
//     );
//     let recv_exact = generate_recv_name(methods_name.clone(), "exact");
//     let recv_exact_with_deadline = generate_recv_name(methods_name.clone(), "exact_with_deadline");
//     let recv_exact_with_timeout = generate_recv_name(methods_name.clone(), "exact_with_timeout");
//
//
//     let expanded = quote! {
//         #[inline(always)]
//         fn #methods_name<'a>(&mut self, buf: &'a mut [u8]) -> #struct_name<'a> {
//             #struct_name::new(self.as_raw_fd(), buf)
//         }
//
//         #[inline(always)]
//         fn #recv_with_deadline<'a>(&mut self, buf: &'a mut [u8], deadline: Instant) -> #struct_name_with_deadline<'a> {
//             #struct_name_with_deadline::new(self.as_raw_fd(), buf, deadline)
//         }
//
//         #[inline(always)]
//         fn #recv_with_timeout<'a>(&mut self, buf: &'a mut [u8], duration: Duration) -> #struct_name_with_deadline<'a> {
//             self.#recv_with_deadline(buf, Instant::now() + duration)
//         }
//
//         #[inline(always)]
//         async fn #poll_recv_buf(&mut self) -> Result<Buffer> {
//             self.poll().await?;
//
//             let mut buf = buffer();
//             buf.set_len_to_cap();
//
//             let received = self.#methods_name(buf.as_mut()).await?;
//             buf.set_len(received);
//
//             Ok(buf)
//         }
//
//         #[inline(always)]
//         async fn #poll_recv_buf_with_deadline(&mut self, deadline: Instant) -> Result<Buffer> {
//             self.poll_with_deadline(deadline).await?;
//
//             let mut buf = buffer();
//             buf.set_len(buf.cap());
//             let received = self.#recv_with_deadline(buf.as_mut(), deadline).await?;
//
//             buf.set_len(received);
//             Ok(buf)
//         }
//
//         #[inline(always)]
//         async fn #poll_recv_buf_with_timeout(&mut self, duration: Duration) -> Result<Buffer> {
//             self.poll_with_timeout(duration).await?;
//
//             let mut buf = buffer();
//             buf.set_len(buf.cap());
//             let received = self.#recv_with_timeout(buf.as_mut(), duration).await?;
//
//             buf.set_len(received);
//             Ok(buf)
//         }
//
//         #[inline(always)]
//         async fn #recv_exact(&mut self, buf: &mut [u8]) -> Result<()> {
//             let mut received = 0;
//
//             while received < buf.len() {
//                 received += self.#methods_name(&mut buf[received..]).await?;
//             }
//
//             Ok(())
//         }
//
//         #[inline(always)]
//         async fn #recv_exact_with_deadline(&mut self, buf: &mut [u8], deadline: Instant) -> Result<()> {
//             let mut received = 0;
//
//             while received < buf.len() {
//                 received += self.#recv_with_deadline(&mut buf[received..], deadline).await?;
//             }
//
//             Ok(())
//         }
//
//         #[inline(always)]
//         async fn #recv_exact_with_timeout(&mut self, buf: &mut [u8], duration: Duration) -> Result<()> {
//             let mut received = 0;
//
//             while received < buf.len() {
//                 received += self.#recv_with_timeout(&mut buf[received..], duration).await?;
//             }
//
//             Ok(())
//         }
//     };
//
//     TokenStream::from(expanded)
// }