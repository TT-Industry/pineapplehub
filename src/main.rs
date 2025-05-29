mod ui;

use crate::ui::show_res;

use ::image::{DynamicImage, imageops::blur};
use iced::{
    Element, Length, Task,
    widget::{button, column, container, horizontal_space, image, row, scrollable},
};
use rfd::AsyncFileDialog;

type EncodedImage = Vec<u8>;

#[derive(Debug, Clone)]
enum Message {
    ChooseFile,
    FileUploaded(EncodedImage),
    Process,
}

#[derive(Default)]
struct Img {
    original: EncodedImage,
    gray: DynamicImage,
    blur: DynamicImage,
}

impl Img {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ChooseFile => Task::perform(upload_img(), Message::FileUploaded),
            Message::FileUploaded(img) => {
                self.original = img;

                Task::none()
            }
            Message::Process => {
                let gray = ::image::load_from_memory(&self.original)
                    .unwrap()
                    .to_luma8();
                self.gray = DynamicImage::ImageLuma8(gray);
                // https://docs.rs/image/0.25.6/src/image/imageops/sample.rs.html#1004
                self.blur = DynamicImage::ImageLuma8(blur(&self.gray.to_luma8(), 7.0));

                Task::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        container(row![
            column![
                button("Choose the image").on_press(Message::ChooseFile),
                image(image::Handle::from_bytes(self.original.clone())),
                button("Do it!").on_press(Message::Process),
            ]
            .spacing(10)
            .width(Length::FillPortion(2)),
            scrollable(column![
                show_res("Gray", &self.gray),
                horizontal_space(),
                show_res("Blur", &self.gray),
            ])
            .width(Length::FillPortion(8))
        ])
        .into()
    }
}

async fn upload_img() -> EncodedImage {
    let file = AsyncFileDialog::new().pick_file().await;

    file.unwrap().read().await
}

fn main() -> iced::Result {
    console_log::init().expect("Initialize logger");
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    iced::application(Img::default, Img::update, Img::view)
        .centered()
        .run()
}
