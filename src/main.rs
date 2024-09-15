use std::{
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
};

#[tokio::main]
async fn main() -> io::Result<()> {
    let (mut tx1, rx1) = oneshot::channel();
    let (tx2, rx2) = oneshot::channel();

    tokio::spawn(async {
        // let _ = tx1.send("one");
        // selcet on the operation and the oneshot's
        // `closed()` notification.
        tokio::select! {
            val = some_operation() => {
                let _ = tx1.send(val);
            }
            _ = tx1.closed() => {
                // `some_operation()` is canceled,
                // the task completes and `tx1` is dropped
            }
        }
    });

    tokio::spawn(async {
        let _ = tx2.send("two");
    });

    tokio::select! {
        val = rx1 => {
            println!("rx1 completed first with {:?}", val);
        }
        val = rx2 => {
            println!("rx2 completed first with {:?}", val);
        }
    }

    // -----------------------
    let (tx3, rx3) = oneshot::channel();

    tokio::spawn(async move {
        tx3.send("done").unwrap();
    });

    tokio::select! {
        socket = TcpStream::connect("localhost:3465") => {
            println!("Socket connected {:?}", socket);
        }
        msg = rx3 => {
            println!("received message first {:?}", msg);
        }
    }

    //-----------------------------------
    let (tx4, rx4) = oneshot::channel();

    tokio::spawn(async move {
        tx4.send(()).unwrap();
    });

    let mut listener = TcpListener::bind("localhost:3466").await?;

    tokio::select! {
    res = async {
            loop {
                let (socket, _) = listener.accept().await?;
                tokio::spawn(async move { process(socket) });
            }

            Ok::<_, io::Error>(())
        } => {
            res?;
        }
        _ = rx4 => {
            println!("terminating accept loop");
        }
    }

    //-----------------------------
    let (tx5, mut rx5) = mpsc::channel::<&str>(128);
    let (tx6, mut rx6) = mpsc::channel::<&str>(128);

    tokio::spawn(async move {
        let _ = tx5.send("5").await;
        let _ = tx6.send("6").await;
    });

    tokio::select! {
        Some(v) = rx5.recv() => {
            println!("Got {:?} from rx1", v);
        }
        Some(v) = rx6.recv() => {
            println!("Got {:?} from rx2", v);
        }
        else => {
            println!("Both channels closed");
        }
    }

    //---------------------------
    channel_race().await;

    //---------------------------
    even_received().await;

    //----------------------------
    even_received_two().await;
    Ok(())
}

async fn some_operation() {
    println!("some operation");
}

fn process(socket: TcpStream) {
    println!("proccessing the socket");
}

struct MySelect {
    rx1: oneshot::Receiver<&'static str>,
    rx2: oneshot::Receiver<&'static str>,
}

impl Future for MySelect {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Poll::Ready(val) = Pin::new(&mut self.rx1).poll(cx) {
            println!("rx1 completed first with {:?}", val);
            return Poll::Ready(());
        }

        if let Poll::Ready(val) = Pin::new(&mut self.rx2).poll(cx) {
            println!("rx2 completed first with {:?}", val);
            return Poll::Ready(());
        }

        Poll::Pending
    }
}

async fn write_socket(data: &[u8], addr: SocketAddr) -> io::Result<()> {
    let mut socket = TcpStream::connect(addr).await?;
    socket.write_all(data).await?;
    Ok::<_, io::Error>(())
}

async fn race(data: &[u8], addr1: SocketAddr, addr2: SocketAddr) -> io::Result<()> {
    tokio::select! {
        Ok(_) = write_socket(data, addr1) => {}
        Ok(_) = async {
            let mut socket = TcpStream::connect(addr2).await?;
            socket.write_all(data).await?;
            Ok::<_, io::Error>(())
        } => {}
        else => {}
    };

    Ok(())
}

// If when select! is evaluated, multiple channels have pending
// messages, only one channel has a value popped.
// All other channels remain untouched, and their messages
// stay in those channels until the next loop iteration.
// No messages are lost.
async fn channel_race() {
    let (_, mut rx1) = mpsc::channel(128);
    let (_, mut rx2) = mpsc::channel(128);
    let (_, mut rx3) = mpsc::channel(128);

    loop {
        let msg = tokio::select! {
            Some(msg) = rx1.recv() => msg,
            Some(msg) = rx2.recv() => msg,
            Some(msg) = rx3.recv() => msg,
            else => { break }
        };

        println!("Got {:?}", msg);
    }

    println!("All channels have been closed.");
}

async fn even_received() {
    let (mut _tx, mut rx) = tokio::sync::mpsc::channel::<i32>(128);

    let operation = action();
    tokio::pin!(operation);

    loop {
        tokio::select! {
            _ = &mut operation => break,
            Some(v) = rx.recv() => {
                if v % 2 == 0 {
                    break;
                }
            }
        }
    }
}

async fn action() {}

async fn action_with_input(input: Option<i32>) -> Option<String> {
    let i = input?;

    Some(i.to_string())
}

async fn even_received_two() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<i32>(128);

    let mut done = false;
    let operation = action_with_input(None);
    tokio::pin!(operation);

    tokio::spawn(async move {
        let _ = tx.send(1).await;
        let _ = tx.send(3).await;
        let _ = tx.send(2).await;
    });

    loop {
        tokio::select! {
            res = &mut operation, if !done => {
                done = true;

                if let Some(v) = res {
                    println!("GOT = {}", v);
                    return;
                }
            }
            Some(v) = rx.recv() => {
                if v % 2 == 0 {
                    // `.set` is a method on `Pin`
                    operation.set(action_with_input(Some(v)));
                    done = false;
                }
            }
        }
    }
}
