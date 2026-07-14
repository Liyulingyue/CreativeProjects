fn main() {
    tauri::Builder::default()
        .setup(|_app| {
            let ready = photo_analyzer::spawn_server(8001, false);

            match ready.recv_timeout(std::time::Duration::from_secs(10)) {
                Ok(Ok(())) => Ok(()),
                Ok(Err(message)) => Err(std::io::Error::other(message).into()),
                Err(_) => Err(std::io::Error::other("后端启动超时").into()),
            }
        })
        .build(tauri::generate_context!())
        .expect("Tauri app build failed")
        .run(|_, _| {});
}