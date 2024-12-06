#[cfg(test)]
mod buffer_tests {
    use crate as orengine;
    use crate::io::FixedBuffer;

    #[orengine::test::test_local]
    fn test_new() {
        let buf = crate::io::Buffer::new(1);
        assert_eq!(buf.capacity(), 1);
    }

    #[orengine::test::test_local]
    fn test_add_len_and_set_len_to_cap() {
        let mut buf = crate::io::Buffer::new(100);
        assert_eq!(buf.len(), 0);

        unsafe { buf.add_len(10) };
        assert_eq!(buf.len(), 10);

        unsafe { buf.add_len(20) };
        assert_eq!(buf.len(), 30);

        buf.set_len_to_capacity();
        assert_eq!(buf.len_u32(), buf.capacity());
    }

    #[orengine::test::test_local]
    fn test_len_and_cap() {
        let mut buf = crate::io::Buffer::new(100);
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.capacity(), 100);

        buf.set_len(10);
        assert_eq!(buf.len(), 10);
        assert_eq!(buf.capacity(), 100);
    }

    #[orengine::test::test_local]
    fn test_resize() {
        let mut buf = crate::io::Buffer::new(100);
        buf.append(&[1, 2, 3]);

        buf.resize(200);
        assert_eq!(buf.capacity(), 200);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);

        buf.resize(50);
        assert_eq!(buf.capacity(), 50);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
    }

    #[orengine::test::test_local]
    fn test_append_and_clear() {
        let mut buf = crate::io::Buffer::new(5);

        buf.append(&[1, 2, 3]);
        // This code checks written
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.capacity(), 5);

        buf.append(&[4, 5, 6]);
        assert_eq!(buf.as_ref(), &[1, 2, 3, 4, 5, 6]);
        assert_eq!(buf.capacity(), 10);

        buf.clear();
        assert_eq!(buf.as_ref(), &[]);
        assert_eq!(buf.capacity(), 10);
    }

    #[orengine::test::test_local]
    fn test_is_empty_and_is_full() {
        let mut buf = crate::io::Buffer::new(5);
        assert!(buf.is_empty());
        buf.append(&[1, 2, 3]);
        assert!(!buf.is_empty());
        buf.clear();
        assert!(buf.is_empty());

        let mut buf = crate::io::Buffer::new(5);
        assert!(!buf.is_full());
        buf.append(&[1, 2, 3, 4, 5]);
        assert!(buf.is_full());
        buf.clear();
        assert!(!buf.is_full());
    }

    #[orengine::test::test_local]
    fn test_index() {
        let mut buf = crate::io::Buffer::new(5);
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
        let buf = crate::io::Buffer::from(b);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 3);

        let mut v = vec![1, 2, 3];
        v.reserve(7);
        let buf = crate::io::Buffer::from(v);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 10);

        let v = vec![1, 2, 3];
        let buf = crate::io::Buffer::from(v.into_boxed_slice());
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 3);
    }
}

#[cfg(test)]
mod buf_pool_tests {
    use crate as orengine;
    use crate::io::{buf_pool, buffer, full_buffer};

    #[orengine::test::test_local]
    fn test_buf_pool() {
        let pool = buf_pool();
        assert_eq!(pool.len(), 0);

        let buf = buffer();
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.capacity(), 4096);
        drop(buf);

        let buf = full_buffer();
        assert_eq!(buf.len(), 4096);
        assert_eq!(buf.capacity(), 4096);
        drop(buf);

        assert_eq!(pool.len(), 1);

        let _buf = pool.get();
        assert_eq!(pool.len(), 0);
    }
}
