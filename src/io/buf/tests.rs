#[cfg(test)]
mod buffer_tests {
    use crate as orengine;

    #[orengine::test::test_local]
    fn test_new() {
        let buf = crate::buf::linux::linux_buffer::LinuxBuffer::new(1);
        assert_eq!(buf.cap(), 1);
    }

    #[orengine::test::test_local]
    fn test_add_len_and_set_len_to_cap() {
        let mut buf = crate::buf::linux::linux_buffer::LinuxBuffer::new(100);
        assert_eq!(buf.len(), 0);

        buf.add_len(10);
        assert_eq!(buf.len(), 10);

        buf.add_len(20);
        assert_eq!(buf.len(), 30);

        buf.set_len_to_cap();
        assert_eq!(buf.len(), buf.cap());
    }

    #[orengine::test::test_local]
    fn test_len_and_cap() {
        let mut buf = crate::buf::linux::linux_buffer::LinuxBuffer::new(100);
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.cap(), 100);

        buf.set_len(10);
        assert_eq!(buf.len(), 10);
        assert_eq!(buf.cap(), 100);
    }

    #[orengine::test::test_local]
    fn test_resize() {
        let mut buf = crate::buf::linux::linux_buffer::LinuxBuffer::new(100);
        buf.append(&[1, 2, 3]);

        buf.resize(200);
        assert_eq!(buf.cap(), 200);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);

        buf.resize(50);
        assert_eq!(buf.cap(), 50);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
    }

    #[orengine::test::test_local]
    fn test_append_and_clear() {
        let mut buf = crate::buf::linux::linux_buffer::LinuxBuffer::new(5);

        buf.append(&[1, 2, 3]);
        // This code checks written
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.cap(), 5);

        buf.append(&[4, 5, 6]);
        assert_eq!(buf.as_ref(), &[1, 2, 3, 4, 5, 6]);
        assert_eq!(buf.cap(), 10);

        buf.clear();
        assert_eq!(buf.as_ref(), &[]);
        assert_eq!(buf.cap(), 10);
    }

    #[orengine::test::test_local]
    fn test_is_empty_and_is_full() {
        let mut buf = crate::buf::linux::linux_buffer::LinuxBuffer::new(5);
        assert!(buf.is_empty());
        buf.append(&[1, 2, 3]);
        assert!(!buf.is_empty());
        buf.clear();
        assert!(buf.is_empty());

        let mut buf = crate::buf::linux::linux_buffer::LinuxBuffer::new(5);
        assert!(!buf.is_full());
        buf.append(&[1, 2, 3, 4, 5]);
        assert!(buf.is_full());
        buf.clear();
        assert!(!buf.is_full());
    }

    #[orengine::test::test_local]
    fn test_index() {
        let mut buf = crate::buf::linux::linux_buffer::LinuxBuffer::new(5);
        buf.append(&[1, 2, 3]);
        assert_eq!(buf[0], 1);
        assert_eq!(buf[1], 2);
        assert_eq!(buf[2], 3);
        assert_eq!(&buf[1..=2], &[2, 3]);
        assert_eq!(&buf[..3], &[1, 2, 3]);
        assert_eq!(&buf[2..], &[3]);
        assert_eq!(&buf[..], &[1, 2, 3]);
    }

    #[orengine::test::test_local]
    fn test_from() {
        let b = Box::new([1, 2, 3]);
        let buf = crate::buf::linux::linux_buffer::LinuxBuffer::from(b);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.cap(), 3);

        let mut v = vec![1, 2, 3];
        v.reserve(7);
        let buf = crate::buf::linux::linux_buffer::LinuxBuffer::from(v);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.cap(), 10);

        let v = vec![1, 2, 3];
        let buf = crate::buf::linux::linux_buffer::LinuxBuffer::from(v.into_boxed_slice());
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.cap(), 3);
    }
}

#[cfg(test)]
mod buf_pool_tests {
    use crate as orengine;
    use crate::buf::{buf_pool, buffer, full_buffer};

    #[orengine::test::test_local]
    fn test_buf_pool() {
        let pool = buf_pool();
        assert!(pool.pool.is_empty());

        let buf = buffer();
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.cap(), 4096);
        drop(buf);

        let buf = full_buffer();
        assert_eq!(buf.len(), 4096);
        assert_eq!(buf.cap(), 4096);
        drop(buf);

        assert_eq!(pool.pool.len(), 1);

        let _buf = pool.get();
        assert!(pool.pool.is_empty());
    }
}
