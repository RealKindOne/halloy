use futures::{stream::BoxStream, Stream, StreamExt};
pub use iced::window::{close, Settings};
use iced::{advanced::graphics::futures::subscription, Point, Size, Subscription};

use data::window::{self, Event};

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn settings() -> Settings {
    Settings::default()
}

#[cfg(target_os = "linux")]
pub fn settings() -> Settings {
    use data::environment;
    use iced::window;

    Settings {
        platform_specific: window::settings::PlatformSpecific {
            application_id: environment::APPLICATION_ID.to_string(),
        },
        ..Default::default()
    }
}

#[cfg(target_os = "macos")]
pub fn settings() -> Settings {
    use iced::window;

    Settings {
        platform_specific: window::settings::PlatformSpecific {
            title_hidden: true,
            titlebar_transparent: true,
            fullsize_content_view: true,
        },
        ..Default::default()
    }
}

#[cfg(target_os = "windows")]
pub fn settings() -> Settings {
    use iced::window;
    use image::EncodableLayout;

    let img = image::load_from_memory_with_format(
        include_bytes!("../assets/logo.png"),
        image::ImageFormat::Png,
    );
    match img {
        Ok(img) => match img.as_rgba8() {
            Some(icon) => Settings {
                icon: window::icon::from_rgba(
                    icon.as_bytes().to_vec(),
                    icon.width(),
                    icon.height(),
                )
                .ok(),
                ..Default::default()
            },
            None => Default::default(),
        },
        Err(_) => Settings {
            ..Default::default()
        },
    }
}

pub fn events() -> Subscription<Event> {
    subscription::from_recipe(Events)
}

enum State<T: Stream<Item = Event>> {
    Idle {
        stream: T,
    },
    Moving {
        stream: T,
        position: window::Position,
    },
    Resizing {
        stream: T,
        size: window::Size,
    },
    MovingAndResizing {
        stream: T,
        position: window::Position,
        size: window::Size,
    },
}

struct Events;

impl subscription::Recipe for Events {
    type Output = Event;

    fn hash(&self, state: &mut subscription::Hasher) {
        use std::hash::Hash;

        std::any::TypeId::of::<Self>().hash(state);
    }

    fn stream(
        self: Box<Self>,
        events: subscription::EventStream,
    ) -> BoxStream<'static, Self::Output> {
        use futures::stream;
        const TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

        let window_events = events.filter_map(|event| {
            futures::future::ready(match event {
                subscription::Event::Interaction {
                    window: _,
                    event: iced::Event::Window(window_event),
                    status: _,
                } => match window_event {
                    iced::window::Event::Moved(Point { x, y }) => {
                        Some(Event::Moved(window::Position::new(x, y)))
                    }
                    iced::window::Event::Resized(Size { width, height }) => {
                        Some(Event::Resized(window::Size::new(width, height)))
                    }
                    _ => None,
                },
                _ => None,
            })
        });

        stream::unfold(
            State::Idle {
                stream: window_events,
            },
            move |state| async move {
                match state {
                    State::Idle { mut stream } => stream.next().await.map(|event| {
                        (
                            vec![],
                            match event {
                                Event::Moved(position) => State::Moving { stream, position },
                                Event::Resized(size) => State::Resizing { stream, size },
                            },
                        )
                    }),
                    State::Moving {
                        mut stream,
                        position,
                    } => {
                        let next_event = tokio::time::timeout(TIMEOUT, stream.next()).await;

                        match next_event {
                            Ok(Some(Event::Moved(position))) => {
                                Some((vec![], State::Moving { stream, position }))
                            }
                            Ok(Some(Event::Resized(size))) => Some((
                                vec![],
                                State::MovingAndResizing {
                                    stream,
                                    position,
                                    size,
                                },
                            )),
                            Err(_) => Some((vec![Event::Moved(position)], State::Idle { stream })),
                            _ => None,
                        }
                    }
                    State::Resizing { mut stream, size } => {
                        let next_event = tokio::time::timeout(TIMEOUT, stream.next()).await;

                        match next_event {
                            Ok(Some(Event::Resized(size))) => {
                                Some((vec![], State::Resizing { stream, size }))
                            }
                            Ok(Some(Event::Moved(position))) => Some((
                                vec![],
                                State::MovingAndResizing {
                                    stream,
                                    position,
                                    size,
                                },
                            )),
                            Err(_) => Some((vec![Event::Resized(size)], State::Idle { stream })),
                            _ => None,
                        }
                    }
                    State::MovingAndResizing {
                        mut stream,
                        position,
                        size,
                    } => {
                        let next_event = tokio::time::timeout(TIMEOUT, stream.next()).await;

                        match next_event {
                            Ok(Some(Event::Moved(position))) => Some((
                                vec![],
                                State::MovingAndResizing {
                                    stream,
                                    position,
                                    size,
                                },
                            )),
                            Ok(Some(Event::Resized(size))) => Some((
                                vec![],
                                State::MovingAndResizing {
                                    stream,
                                    position,
                                    size,
                                },
                            )),
                            Err(_) => Some((
                                vec![Event::Moved(position), Event::Resized(size)],
                                State::Idle { stream },
                            )),
                            _ => None,
                        }
                    }
                }
            },
        )
        .map(stream::iter)
        .flatten()
        .boxed()
    }
}
