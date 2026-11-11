use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum Request<'a> {
    #[serde(rename = "create_session")]
    CreateSession { username: &'a str },
    #[serde(rename = "post_auth_message_response")]
    PostAuthMessageResponse {
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<&'a str>,
    },
    #[serde(rename = "start_session")]
    StartSession {
        cmd: &'a [String],
        env: &'a [String],
    },
    #[serde(rename = "cancel_session")]
    CancelSession,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        error_type: String,
        #[serde(default)]
        description: String,
    },
    #[serde(rename = "auth_message")]
    AuthMessage {
        auth_message_type: String,
        #[allow(dead_code)]
        auth_message: String,
    },
}

pub trait GreetdClient {
    fn request(&mut self, req: &Request) -> Result<Response, String>;
}

pub struct RealGreetd {
    stream: UnixStream,
}

impl RealGreetd {
    pub fn connect() -> Result<Self, String> {
        let sock_path = std::env::var("GREETD_SOCK")
            .map_err(|_| "Переменная окружения GREETD_SOCK не найдена".to_string())?;
        if !Path::new(&sock_path).exists() {
            return Err(format!("Сокет greetd не найден: {}", sock_path));
        }
        let stream = UnixStream::connect(sock_path)
            .map_err(|e| format!("Не удалось подключиться к сокету greetd: {}", e))?;
        Ok(Self { stream })
    }

    fn send(&mut self, req: &Request) -> Result<(), String> {
        let json_bytes = serde_json::to_vec(req)
            .map_err(|e| format!("Ошибка сериализации запроса: {}", e))?;
        let len = (json_bytes.len() as u32).to_ne_bytes();
        self.stream.write_all(&len)
            .map_err(|e| format!("Ошибка записи длины запроса: {}", e))?;
        self.stream.write_all(&json_bytes)
            .map_err(|e| format!("Ошибка записи тела запроса: {}", e))?;
        self.stream.flush()
            .map_err(|e| format!("Ошибка сброса сокета: {}", e))?;
        Ok(())
    }

    fn recv(&mut self) -> Result<Response, String> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf)
            .map_err(|e| format!("greetd закрыл соединение или ошибка заголовка: {}", e))?;
        let len = u32::from_ne_bytes(len_buf) as usize;

        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data)
            .map_err(|e| format!("Не удалось прочитать ответ greetd: {}", e))?;

        serde_json::from_slice(&data)
            .map_err(|e| format!("Ошибка парсинга ответа greetd: {}", e))
    }
}

impl GreetdClient for RealGreetd {
    fn request(&mut self, req: &Request) -> Result<Response, String> {
        self.send(req)?;
        self.recv()
    }
}

pub struct DemoGreetd {
    _dummy: bool,
}

impl DemoGreetd {
    pub fn new() -> Self {
        Self { _dummy: true }
    }
}

impl GreetdClient for DemoGreetd {
    fn request(&mut self, req: &Request) -> Result<Response, String> {
        std::thread::sleep(std::time::Duration::from_millis(300));
        match req {
            Request::CreateSession { .. } => Ok(Response::AuthMessage {
                auth_message_type: "secret".to_string(),
                auth_message: "Password: ".to_string(),
            }),
            Request::PostAuthMessageResponse { response } => {
                if *response == Some("demo") {
                    Ok(Response::Success)
                } else {
                    Ok(Response::Error {
                        error_type: "auth_error".to_string(),
                        description: "auth failed".to_string(),
                    })
                }
            }
            Request::StartSession { cmd, .. } => {
                println!("DEMO start_session: {:?}", cmd);
                Ok(Response::Success)
            }
            Request::CancelSession => Ok(Response::Success),
        }
    }
}

/// Выполняет полный цикл аутентификации greetd
pub fn authenticate(
    client: &mut dyn GreetdClient,
    username: &str,
    password: &str,
    cmd: &[String],
) -> Result<(), String> {
    let mut resp = client.request(&Request::CreateSession { username })?;

    while let Response::AuthMessage { auth_message_type, .. } = resp {
        let answer = match auth_message_type.as_str() {
            "secret" => Some(password),
            "visible" => Some(""),
            _ => None,
        };
        resp = client.request(&Request::PostAuthMessageResponse { response: answer })?;
    }

    match resp {
        Response::Success => {
            let start_resp = client.request(&Request::StartSession {
                cmd,
                env: &[],
            })?;
            match start_resp {
                Response::Success => Ok(()),
                Response::Error { description, .. } => {
                    let _ = client.request(&Request::CancelSession);
                    Err(if description.is_empty() {
                        "Не удалось запустить сессию".to_string()
                    } else {
                        description
                    })
                }
                _ => {
                    let _ = client.request(&Request::CancelSession);
                    Err("Неожиданный ответ greetd при запуске сессии".to_string())
                }
            }
        }
        Response::Error { error_type, description } => {
            let _ = client.request(&Request::CancelSession);
            if error_type == "auth_error" {
                Err("Неверный пароль".to_string())
            } else if !description.is_empty() {
                Err(description)
            } else {
                Err("Ошибка входа".to_string())
            }
        }
        _ => {
            let _ = client.request(&Request::CancelSession);
            Err("Неожиданный ответ greetd".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_demo_authenticate_success() {
        let mut client = DemoGreetd::new();
        let cmd = vec!["start-hyprland".to_string()];
        let res = authenticate(&mut client, "ineron", "demo", &cmd);
        assert!(res.is_ok());
    }

    #[test]
    fn test_demo_authenticate_wrong_password() {
        let mut client = DemoGreetd::new();
        let cmd = vec!["start-hyprland".to_string()];
        let res = authenticate(&mut client, "ineron", "wrong", &cmd);
        assert_eq!(res.unwrap_err(), "Неверный пароль");
    }

    #[test]
    fn test_real_greetd_unix_socket() {
        use std::os::unix::net::UnixListener;
        let sock_path = "/tmp/test-greetd-socket.sock";
        let _ = std::fs::remove_file(sock_path);

        let listener = UnixListener::bind(sock_path).unwrap();

        let srv_handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();

            fn send_msg(stream: &mut std::os::unix::net::UnixStream, msg: &serde_json::Value) {
                let bytes = serde_json::to_vec(msg).unwrap();
                let len = (bytes.len() as u32).to_ne_bytes();
                stream.write_all(&len).unwrap();
                stream.write_all(&bytes).unwrap();
                stream.flush().unwrap();
            }

            fn recv_msg(stream: &mut std::os::unix::net::UnixStream) -> serde_json::Value {
                let mut len_buf = [0u8; 4];
                stream.read_exact(&mut len_buf).unwrap();
                let len = u32::from_ne_bytes(len_buf) as usize;
                let mut data = vec![0u8; len];
                stream.read_exact(&mut data).unwrap();
                serde_json::from_slice(&data).unwrap()
            }

            // 1. create_session
            let msg1 = recv_msg(&mut stream);
            assert_eq!(msg1["type"], "create_session");
            assert_eq!(msg1["username"], "ineron");

            // send auth_message
            send_msg(&mut stream, &serde_json::json!({
                "type": "auth_message",
                "auth_message_type": "secret",
                "auth_message": "Password: "
            }));

            // 2. post_auth_message_response
            let msg2 = recv_msg(&mut stream);
            assert_eq!(msg2["type"], "post_auth_message_response");
            assert_eq!(msg2["response"], "correct_pass");

            // send success
            send_msg(&mut stream, &serde_json::json!({ "type": "success" }));

            // 3. start_session
            let msg3 = recv_msg(&mut stream);
            assert_eq!(msg3["type"], "start_session");
            assert_eq!(msg3["cmd"], serde_json::json!(["start-hyprland"]));

            send_msg(&mut stream, &serde_json::json!({ "type": "success" }));
        });

        unsafe {
            std::env::set_var("GREETD_SOCK", sock_path);
        }

        let mut client = RealGreetd::connect().expect("Connection to mock greetd unix socket failed");
        let cmd = vec!["start-hyprland".to_string()];
        let res = authenticate(&mut client, "ineron", "correct_pass", &cmd);

        assert!(res.is_ok());
        srv_handle.join().unwrap();
        let _ = std::fs::remove_file(sock_path);
    }
}
