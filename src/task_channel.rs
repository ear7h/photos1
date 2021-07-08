
use tokio::runtime::Runtime;

use crate::App;
use crate::BufBufWrite;


crate struct TaskChannel<A : App> {
    // TODO: unbounded sender or increase bound size
    sender : tokio::sync::mpsc::Sender<A::Msg>,
    _rt : Runtime,
}

impl <A : App> TaskChannel<A> {
    crate fn new(app : &'static A, model : BufBufWrite<A::Model>) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("photos-workers")
            .build()
            .unwrap();

        let (sender, mut recv) = tokio::sync::mpsc::channel(1);

        rt.spawn(async move {
            loop {
                println!("waiting for message");
                let msg = if let Some(msg) = recv.recv().await {
                    msg
                } else {
                    break
                };

                println!("got msg : {:?}", msg);

                if let Err(err) = app.update(&model, msg).await {
                    app.handle_error(err)
                }
            }
        });

        Self{sender, _rt : rt}
    }

    crate fn send(&self, msg : A::Msg) {
        println!("sending msg : {:?}", msg);
        self.sender
            .blocking_send(msg)
            .unwrap();
    }
}
