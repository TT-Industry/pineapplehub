use ::image::DynamicImage;
use iced::{
    Element,
    widget::{column, image, text},
};

use crate::Message;

pub(crate) fn show_res<'a>(title: &'a str, img: &'a DynamicImage) -> Element<'a, Message> {
    column![
        text(title),
        image(image::Handle::from_rgba(
            img.width(),
            img.height(),
            img.to_rgba8().into_raw()
        ))
    ]
    .into()
}
