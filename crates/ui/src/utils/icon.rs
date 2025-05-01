// This needs the following added to Cargo.toml:
//
//      egui_extras = { version = "*", features = ["all_loaders"] }
//      image = { version = "0.25", features = ["jpeg", "png"] } # Add the types you want support
//
// And Rust 2024 Nightly, which you decided not to use just yet purely to show an icon.

// pub fn load_icon() -> egui::IconData {
//     let (icon_rgba, icon_width, icon_height) = {
//         let icon = include_bytes!("../../../_docs/halo_logo.png");
//         let image = image::load_from_memory(icon)
//             .expect("Failed to open icon path")
//             .into_rgba8();
//         let (width, height) = image.dimensions();
//         let rgba = image.into_raw();
//         (rgba, width, height)
//     };

//     egui::IconData {
//         rgba: icon_rgba,
//         width: icon_width,
//         height: icon_height,
//     }
// }
