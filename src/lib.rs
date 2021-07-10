use std::cell::UnsafeCell;
use std::io::{self, Error, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use byteorder::{ByteOrder, LittleEndian};

fn send_string(message: &str, stream: &mut TcpStream) -> io::Result<()> {
    let mut packet: Vec<u8> = vec![0x1];
    packet.extend(&message.len().to_le_bytes());
    stream.write(&packet)?;
    let mut packet: Vec<u8> = vec![0x2];
    packet.extend(message.as_bytes());
    stream.write(&packet)?;
    Ok(())
}

fn receive_string(stream: &mut TcpStream) -> io::Result<String> {
    let mut buf = Vec::with_capacity(0x200);
    unsafe {
        buf.set_len(buf.capacity());
    }
    stream.read(&mut buf.as_mut_slice()[..1])?;
    let id = buf[0];
    if id != 0x1 {
        send_string("Receiving error: Invalid string packet encounterd.", stream)?;
        return io::Result::Err(Error::new(
            ErrorKind::InvalidData,
            "String length packet began with wrong ID"
        ));
    }
    stream.read(&mut buf.as_mut_slice()[..8])?;
    let length = LittleEndian::read_u64(&buf.as_slice()[..8]) as usize;
    buf.reserve(length);
    unsafe {
        buf.set_len(length);
    }
    stream.read(&mut buf.as_mut_slice()[..1])?;
    let id = buf[0];
    if id != 0x2 {
        send_string("Receiving error: Invalid string packet encountered.", stream)?;
        return io::Result::Err(Error::new(
            ErrorKind::InvalidData,
            "String data packet began with wrong ID"
        ));
    }
    let mut read = 0;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
    while read != length {
        read += match stream.read(&mut buf.as_mut_slice()[read..]) {
            Ok(len) => len,
            Err(_) => {
                break;
            }
        }
    }
    stream.set_read_timeout(None)?;
    unsafe { Ok(String::from_utf8_unchecked(buf)) }
}

fn on_receive(_: Vec<String>) {}

static mut RECEIVER: Receiver = Receiver::CLIStyle(on_receive);
static mut COMM_CHANNEL: Option<Arc<UnsafeCell<TcpStream>>> = None;

pub fn send(message: &str) -> bool {
    match unsafe { COMM_CHANNEL.clone() } {
        Some(channel) => {
            match send_string(message, unsafe { &mut *channel.get() }) {
                Ok(_) => true,
                Err(_) => false
            }
        },
        None => false
    }
}

pub enum Receiver {
    CLIStyle(fn(Vec<String>)),
    Normal(fn(String))
}

pub fn set_on_receive(receiver: Receiver) {
    unsafe {
        RECEIVER = receiver;
    }
}

pub fn start_client(ip: &str, port: u16) {
    loop {
        println!("Attempting to connect to server...");
        let comm_channel;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if let Ok(c) = TcpStream::connect(&format!("{}:{}", ip, port)) {
                comm_channel = c;
                println!("Connected to server.");
                break;
            }
        }
        let comm_channel = Arc::new(UnsafeCell::new(comm_channel));
        unsafe {
            COMM_CHANNEL = Some(comm_channel.clone());
        }
        let comm_channel = unsafe { &mut *comm_channel.get() };
    
        loop {
            match receive_string(comm_channel) {
                Ok(s) => {
                    match unsafe { &RECEIVER } {
                        Receiver::CLIStyle(func) => {
                            let args = s.split_ascii_whitespace().filter_map(|x| {
                                let x = x.trim();
                                if x.is_empty() {
                                    None
                                } else {
                                    Some(String::from(x))
                                }
                            }).collect();
                            func(args)
                        },
                        Receiver::Normal(func) => {
                            func(s)
                        }
                    }
                },
                Err(_) => {
                    println!("Failed to receive string. Disconnecting from server.");
                    break;
                }
            }
        }
    }
}

pub fn is_connected() -> bool {
    unsafe {
        COMM_CHANNEL.is_some()
    }
}

pub fn start_server(host_name: &str, port: u16) {
    println!("Opening skyline-communicate server...");
    let listener;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Ok(c) = TcpListener::bind(&format!("0.0.0.0:{}", port)) {
            listener = c;
            println!("skyline-communicate server has started.");
            break;
        }
    }

    loop {
        println!("Awaiting client...");
        let comm_channel = Arc::new(UnsafeCell::new(listener.accept().unwrap().0));
        unsafe {
            COMM_CHANNEL = Some(comm_channel.clone());
        }
        let comm_channel = unsafe { &mut *comm_channel.get() };
        println!("Client connected.");
        let _ = send_string(format!("Connected to server. Host: {}", host_name).as_str(), comm_channel);
        loop {
            match receive_string(comm_channel) {
                Ok(s) => {
                    match unsafe { &RECEIVER } {
                        Receiver::CLIStyle(func) => {
                            let args = s.split_ascii_whitespace().filter_map(|x| {
                                let x = x.trim();
                                if x.is_empty() {
                                    None
                                } else {
                                    Some(String::from(x))
                                }
                            }).collect();
                            func(args)
                        },
                        Receiver::Normal(func) => {
                            func(s)
                        }
                    }
                },
                Err(_) => {
                    let _ = send_string("Failed to read message. Disconnecting from server.", comm_channel);
                    break;
                }
            }
        }
        unsafe {
            COMM_CHANNEL = None;
        }
    }
}