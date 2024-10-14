use std::future::Future;
use std::time::Duration;

pub(crate) struct Bencher {
    n: usize,
    name: String,
}

impl Bencher {
    #[inline(always)]
    fn new(name: &str) -> Self {
        Self {
            n: 1,
            name: name.to_string(),
        }
    }

    #[inline(always)]
    pub(crate) fn iter<F: Fn()>(&mut self, f: F) {
        // region warm up

        loop {
            let start = std::time::Instant::now();
            for _ in 0..self.n {
                f();
            }
            let elapsed = start.elapsed();
            if elapsed >= Duration::from_millis(10) {
                if self.n > 10 {
                    self.n = (self.n as f64
                        / (elapsed.as_nanos() as f64 / Duration::from_millis(10).as_nanos() as f64))
                        as usize;
                } else {
                    println!("{:?} takes a lot of time!", self.name);
                }

                break;
            }

            self.n *= 30;
        }

        // end region

        let mut total = 0;

        for _ in 0..1000 {
            let start = std::time::Instant::now();
            for _ in 0..self.n {
                f();
            }
            let elapsed = start.elapsed();
            total += elapsed.as_nanos();
        }

        let average_nanos = (total / 1000) / self.n as u128;

        match average_nanos {
            0..1_000 => {
                println!("{:?} took {} nanoseconds", self.name, average_nanos);
            }
            1_000..1_000_000 => {
                let average_micros = average_nanos / 1_000;
                println!(
                    "{:?} took {}.{} microseconds",
                    self.name,
                    average_micros,
                    (average_nanos % 1_000) / 100
                );
            }
            _ => {
                let average_millis = average_nanos / 1_000_000;
                println!(
                    "{:?} took {}.{} milliseconds",
                    self.name,
                    average_millis,
                    (average_nanos % 1_000_000) / 1_000
                );
            }
        }
    }

    #[inline(always)]
    pub(crate) async fn iter_async<T, Fut: Future<Output = T>, F: Fn() -> Fut>(&mut self, f: F) {
        // region warm up

        loop {
            let start = std::time::Instant::now();
            for _ in 0..self.n {
                f().await;
            }
            let elapsed = start.elapsed();
            if elapsed >= Duration::from_millis(3) {
                if self.n > 10 {
                    self.n = (self.n as f64
                        / (elapsed.as_nanos() as f64 / Duration::from_millis(10).as_nanos() as f64))
                        as usize;
                } else {
                    println!("{:?} takes a lot of time!", self.name);
                }

                break;
            }

            self.n *= 30;
        }

        // end region

        let mut total = 0;

        for _ in 0..1000 {
            let start = std::time::Instant::now();
            for _ in 0..self.n {
                f().await;
            }
            let elapsed = start.elapsed();
            total += elapsed.as_nanos();
        }

        let average_nanos = (total / 1000) / self.n as u128;

        match average_nanos {
            0..1_000 => {
                println!("{:?} took {} nanoseconds", self.name, average_nanos);
            }
            1_000..1_000_000 => {
                let average_micros = average_nanos / 1_000;
                println!(
                    "{:?} took {}.{} microseconds",
                    self.name,
                    average_micros,
                    (average_nanos % 1_000) / 100
                );
            }
            _ => {
                let average_millis = average_nanos / 1_000_000;
                println!(
                    "{:?} took {}.{} milliseconds",
                    self.name,
                    average_millis,
                    (average_nanos % 1_000_000) / 1_000
                );
            }
        }
    }
}

pub(crate) fn bench<F: FnOnce(Bencher)>(name: &str, f: F) {
    let b = Bencher::new(name);
    f(b);
}
