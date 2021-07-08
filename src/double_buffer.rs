use std::sync::{
    Arc,
    Weak,
    Mutex,
    MutexGuard,
};

pub struct BufBuf<T> {
    current : Arc<Mutex<T>>,
    next : Arc<Mutex<Option<Arc<Mutex<T>>>>>,
}

impl<T> BufBuf<T> {
    pub fn new(v : T) -> Self {
        BufBuf{
            current : Arc::new(Mutex::new(v)),
            next : Arc::new(Mutex::new(None)),
        }
    }

    pub fn lock(&self) -> MutexGuard<T> {
        self.current.lock().unwrap()
    }

    pub fn new_write(&self) -> BufBufWrite<T> {
        BufBufWrite{
            next: Arc::clone(&self.next),
        }
    }

    // the first param is the old value and second param is the new value
    pub fn swap<F : FnOnce(&mut T, &mut T)>(&mut self, f : F) {
        let new_opt = self.next.lock().unwrap().take();

        match new_opt {
            None => {},
            Some(mut new) => {
                std::mem::swap(&mut new, &mut self.current);
                let old = new;
                f(&mut old.lock().unwrap(), &mut self.current.lock().unwrap());
            }
        }
    }
}

pub struct BufBufWrite<T> {
    next : Arc<Mutex<Option<Arc<Mutex<T>>>>>,
}

impl<T> Clone for BufBufWrite<T> {
    fn clone(&self) -> BufBufWrite<T> {
        BufBufWrite{
            next : Arc::clone(&self.next),
        }
    }
}


impl<T> BufBufWrite<T> {
    pub fn set_next(&self, v : T) -> Weak<Mutex<T>> {
        let next = Arc::new(Mutex::new(v));
        let ret = Arc::downgrade(&next);
        *self.next.lock().unwrap() = Some(next);
        ret
    }
}

