use std::{collections::HashMap, os::fd::RawFd};
use zbus::{
    dbus_proxy,
    export::futures_util::TryStreamExt,
    zvariant::{Fd, ObjectPath, OwnedObjectPath, Value},
    Connection, MessageStream, MessageType, Result,
};

#[dbus_proxy(
    interface = "org.freedesktop.portal.ScreenCast",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait ScreenCast {
    async fn create_session(&self, options: HashMap<&str, Value<'_>>) -> Result<OwnedObjectPath>;
    async fn open_pipe_wire_remote(
        &self,
        session_handle: ObjectPath<'_>,
        options: HashMap<&str, Value<'_>>,
    ) -> Result<Fd>;
    async fn select_sources(
        &self,
        session_handle: ObjectPath<'_>,
        options: HashMap<&str, Value<'_>>,
    ) -> Result<OwnedObjectPath>;
    async fn start(
        &self,
        session_handle: ObjectPath<'_>,
        parent_window: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> Result<OwnedObjectPath>;
}

#[async_std::main]
async fn main() -> Result<()> {
    let connection = Connection::session().await?;
    pipewire::init();

    let screencast_proxy = ScreenCastProxy::new(&connection).await?;

    screencast_proxy
        .create_session(HashMap::from([
            ("handle_token", Value::from("bluerecorder_1")),
            ("session_handle_token", Value::from("bluerecorder_1")),
        ]))
        .await?;

    let mut stream = MessageStream::from(connection);
    let mut response_session_handle: String = String::default();

    while let Some(msg) = stream.try_next().await? {
        match msg.message_type() {
            MessageType::Signal => {
                println!("\n\nSignal message: {:?}", msg);
                let (_, response) = msg.body::<(u32, HashMap<&str, Value>)>()?;
                println!("\n\nSignal response: {:?}", response);

                if response.len() > 0 {
                    if response.contains_key("session_handle") {
                        response_session_handle = response
                            .get("session_handle")
                            .unwrap()
                            .clone()
                            .downcast::<String>()
                            .expect("cannot down cast");
                        println!("response_session_handle: {:?}", response_session_handle);
                        let screencast_select_sources = screencast_proxy
                            .select_sources(
                                ObjectPath::try_from(response_session_handle.clone())?,
                                HashMap::from([("handle_token", Value::from("bluerecorder_1"))]),
                            )
                            .await?;
                        println!("screencast_select_sources: {screencast_select_sources}");

                        let screencast_start = screencast_proxy
                            .start(
                                ObjectPath::try_from(response_session_handle.clone())?,
                                "parent_window",
                                HashMap::from([("handle_token", Value::from("bluerecorder_1"))]),
                            )
                            .await?;
                        println!("screencast_start: {screencast_start}");
                    }

                    if response.contains_key("streams") {
                        println!(
                            "\n\nSignal response (stream): {:?}",
                            response.get("streams").unwrap()
                        );

                        let screencast_pipe_wire = screencast_proxy
                            .open_pipe_wire_remote(
                                ObjectPath::try_from(response_session_handle.clone())?,
                                HashMap::from([("handle_token", Value::from("bluerecorder_1"))]),
                            )
                            .await?;
                        println!("screencast_pipe_wire: {screencast_pipe_wire}");

                        let mainloop =
                            pipewire::MainLoop::new().expect("Failed to create Pipewire Mainloop");
                        let pipewire_context = pipewire::Context::new(&mainloop)
                            .expect("Failed to create new pipewire context");
                        let pipewire_core = pipewire_context
                            .connect_fd(
                                RawFd::from(
                                    screencast_pipe_wire.to_string().parse::<i32>().unwrap(),
                                ),
                                None
                            )
                            .expect("Cannot connect to pipewire file descriper");
                        let _registry_listener = pipewire_core
                            .add_listener_local()
                            .done(|_, _| println!("done"))
                            .error(|_, _, _, _| println!("error"))
                            .info(|info| println!("info {:?}", info))
                            .register();
                            
                        mainloop.run();
                        // pipewire_core.
                    }
                }
            }
            MessageType::MethodReturn => {
                println!("\n\nMethodReturn message: {:?}", msg);
            }
            _ => {
                println!("\n\nUnkown message: {:?}", msg);
            }
        }
    }

    Ok(())
}
