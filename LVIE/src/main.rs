#![allow(non_snake_case)]
slint::include_modules!();

use i_slint_backend_winit::WinitWindowAccessor;
use image::{RgbImage, GenericImageView};
use LVIElib::blurs::boxblur::FastBoxBlur;
use LVIElib::blurs::gaussianblur::FastGaussianBlur;
use crate::img_processing::crop;
use img_processing::{build_low_res_preview, collect_histogram_data};
use slint::{Image, Rgb8Pixel, SharedPixelBuffer, SharedString, Weak};

use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::{thread, time};

use rfd::FileDialog;

use itertools::Itertools;

mod history;
mod img_processing;
mod loading;
mod core;

use crate::core::Core;

fn maximize_ui(ui: LVIE) {
    ui.window()
        .with_winit_window(|winit_window: &i_slint_backend_winit::winit::window::Window| {
            winit_window.set_maximized(true);
            winit_window.set_title("LVIE");
        })
        .expect("Failed to use winit!");
}

fn _create_svg_path(buff: &RgbImage) -> [SharedString; 3] {
    let hist = collect_histogram_data(&buff);
    let mut _v: Vec<SharedString> = Vec::new();
    for cmp in hist {
        let scale_factor: u32 = 1;
        let max_value: &u32 = &(cmp.values().max().unwrap() / scale_factor);

        let mut s_out: String = String::from(format!("M 0 {}", max_value));

        for k in cmp.keys().sorted() {
            s_out.push_str(&format!(
                " L {} {}",
                {
                    if k == &0u8 {
                        0u32
                    } else {
                        ((*k as f32) * (*max_value as f32 / 255f32)).round() as u32
                    }
                },
                max_value - (cmp.get(k).unwrap() / scale_factor)
            ));
        }

        s_out.push_str(&format!(" L {max_value} {max_value} Z"));
        _v.push(s_out.into());
    }

    [
        _v.get(0).unwrap().clone(),
        _v.get(1).unwrap().clone(),
        _v.get(2).unwrap().clone(),
    ]
}

#[allow(unreachable_code)]
fn main() {
    const WINIT_BACKEND: bool = if cfg!(windows) { true } else { false };

    let CORE = Core::init(core::CoreBackends::GPU);

    if WINIT_BACKEND {
        slint::platform::set_platform(Box::new(i_slint_backend_winit::Backend::new().unwrap()))
            .expect("Failed to set winit backend!");
    }

    let Window: LVIE = LVIE::new().unwrap();

    let loaded_image = Arc::new(Mutex::new(image::RgbImage::new(0, 0)));
    let full_res_processed = Arc::new(Mutex::new(image::RgbImage::new(0, 0)));
    let preview = Arc::new(Mutex::new(image::RgbImage::new(0, 0)));
    let zoom = Arc::new(Mutex::new((0f32, 0f32, 1f32)));

    // CALLBACKS:
    // open image:
    let img_weak = Arc::clone(&loaded_image);
    let prev_weak = Arc::clone(&full_res_processed);
    let low_res_weak = Arc::clone(&preview);
    let Window_weak = Window.as_weak();
    Window
        .global::<ToolbarCallbacks>()
        .on_open_file_callback(move || {
            // get the file with native file dialog
            let fd = FileDialog::new()
                .add_filter("jpg", &["jpg", "jpeg", "png"])
                .pick_file();
            let binding = fd.unwrap();

            let img =
                image::open(binding.as_path().to_str().unwrap()).expect("Failed to open the image");
            let (real_w, real_h) = img.dimensions();

            // store the image
            let mut _mt = img_weak.lock().expect("Cannot lock mutex");
            *_mt = img.to_rgb8();

            // set the full res preview
            let mut _prev = prev_weak.lock().unwrap();
            *_prev = img.to_rgb8();

            let lrw = Arc::clone(&low_res_weak);
            Window_weak
                .upgrade_in_event_loop(move |Window| {
                    // build the low resolution preview based on the effective sizes on the screen
                    let nw: u32 = Window.get_image_space_size_width().round() as u32;
                    let nh: u32 = (real_h * nw) / real_w;
                    
                    let mut _low_res = lrw.lock().expect("Failed to lock");
                    *_low_res = build_low_res_preview(&img.to_rgb8(), nw, nh);
                    
                    // loading the image into the UI
                    let pix_buf = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(
                        &_low_res,
                        _low_res.width(),
                        _low_res.height(),
                    );

                    // create the histogram and update the UI
                    let ww = Window.as_weak();
                    thread::spawn(move || {
                        let path = _create_svg_path(&img.to_rgb8());
                        ww.upgrade_in_event_loop(move |window| {
                            window.set_svg_path(path.into());
                        })
                        .expect("Failed to run in event loop");
                    });
                    Window.set_image(Image::from_rgb8(pix_buf));
                })
                .expect("Failed to call from event loop");
        });

    // close window: (quit the slint event loop)
    Window
        .global::<ToolbarCallbacks>()
        .on_close_window_callback(|| {
            slint::quit_event_loop().expect("Failed to stop the event loop");
        });

    //reset
    let img_weak = Arc::clone(&loaded_image);
    let prev_weak = Arc::clone(&full_res_processed);
    let low_res_weak = Arc::clone(&preview);
    let Window_weak = Window.as_weak();
    Window.global::<ScreenCallbacks>().on_reset(move || {
        // restore all the previews to the original image
        let img = img_weak.lock().unwrap().deref().clone();
        let (real_w, real_h) = img.dimensions();
        let mut _prev = prev_weak.lock().unwrap();
        *_prev = img.clone();
        let lrw = Arc::clone(&low_res_weak);
        
        Window_weak.upgrade_in_event_loop(move |Window: LVIE| {
            Window.set_image(
                Image::from_rgb8(
            SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(&img, img.width(), img.height())));

            // re-build the preview based on the effective screen sizes
            let nw: u32 = Window.get_image_space_size_width().round() as u32;
            let nh: u32 = (real_h * nw) / real_w;
                    
            let mut _low_res = lrw.lock().expect("Failed to lock");
            *_low_res = build_low_res_preview(&img, nw, nh);

            let ww = Window.as_weak();
            thread::spawn(move || {
                let path = _create_svg_path(&img);
                ww.upgrade_in_event_loop(move |window| {
                    window.set_svg_path(path.into());
                })
                .expect("Failed to run in event loop");
            });
        }).expect("Failed to call event loop");
    });

    // handle the zoom
    let zoom_w = Arc::clone(&zoom);
    let prev_weak = Arc::clone(&full_res_processed);
    let low_res_prev = Arc::clone(&preview);
    let Window_weak = Window.as_weak();
    Window.global::<ScreenCallbacks>().on_preview_click(move|width: f32, height: f32, x: f32, y: f32| {

        // check the aviability of the full resolution image
        // if not, utilize temporary the low resolution one
        let img = if prev_weak.try_lock().is_ok() { 
            prev_weak.lock().unwrap()
        } else { 
            low_res_prev.lock().unwrap()
        };

        // check if there is an image loaded
        if img.dimensions() == (0, 0) { return; }
        
        let mut zoom = zoom_w.lock().unwrap();
        let (img_w, img_h) = (*img).dimensions();

        // zoomed_rectangle_width / image_width  
        let mut prop: f32 = zoom.2;

        // retrive the current zoomed rectange sizes
        let real_w = (img_w as f32 * prop) as u32;
        let real_h = (img_h as f32 * prop) as u32;

        // compute the new zoom rectangle sizes
        prop = prop - (0.1 * (prop / 2f32));
        let new_width = (real_w as f32 * prop) as u32;
        let new_height = (real_h * new_width) / real_w;
        zoom.2 = prop;

        let mut pos:(u32, u32) = (0u32, 0u32);

        // get the x and y coordinates of the click into the real image 
        let coefficient = real_w / (width.round() as u32);
        let adjustement: u32 = (height.round() as u32 - (real_h * width.round() as u32) / real_w) / 2;

        let x = (x.round() as u32) * coefficient;
        let y = ({
            if adjustement <= (y.round() as u32) { y.round() as u32 - adjustement } else { 0u32 }
        }) * coefficient;
        
        // centering the rectangle x
        if x < (new_width / 2) {
            pos.0 = (img_w as f32 * zoom.0) as u32;
        } else if x > real_w - (new_width / 2) {
            pos.0 = (img_w as f32 * zoom.0) as u32 + real_w - new_width;
        } else {
            pos.0 = (img_w as f32 * zoom.0) as u32 + x - (new_width / 2);
        }

        // centering the rectangle y
        if y < (new_height / 2) {
            pos.1 = (img_h as f32 * zoom.1) as u32;
        } else if y > real_h - (new_height / 2) {
            pos.1 = (img_h as f32 * zoom.1) as u32 + real_h - new_height;
        } else {
            pos.1 = (img_h as f32 * zoom.1) as u32 + y - (new_height / 2);
        }

        // update the position
        zoom.0 = pos.0 as f32 / img_w as f32;
        zoom.1 = pos.1 as f32 / img_h as f32;

        // crop and display the image
        let preview = crop(&img.deref(), pos.0, pos.1, new_width, new_height);
    
        let pix_buf = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(
            &preview,
            preview.width(),
            preview.height(),
        );

        Window_weak.upgrade_in_event_loop(|Window: LVIE| Window.set_image(Image::from_rgb8(pix_buf))).expect("Failed to call event loop");

    });

    //saturation
    let prev_weak = Arc::clone(&full_res_processed);
    let low_res_weak = Arc::clone(&preview);
    let Window_weak = Window.as_weak();
    Window
        .global::<ScreenCallbacks>()
        .on_add_saturation(move |value: f32| {
            /* Window_weak
                .upgrade_in_event_loop(|w| w.global::<ScreenCallbacks>().invoke_reset())
                .expect("failed to reset"); */
            let mut prev = low_res_weak.lock().unwrap();
            *prev = img_processing::saturate(&(*prev), value.clone());

            let pix_buf = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(
                &prev.deref(),
                prev.deref().width(),
                prev.deref().height(),
            );

            Window_weak.upgrade_in_event_loop(move |Window: LVIE| {
                Window.set_image(Image::from_rgb8(pix_buf));
            }).expect("Failed to call event loop");

            let pw = prev_weak.clone();
            thread::spawn(move || {
                let mut frp = pw.lock().unwrap();
                *frp = img_processing::saturate(frp.deref(), value);
            });
            
        });

    // apply filter
    let prev_weak = Arc::clone(&full_res_processed);
    let low_res_weak = Arc::clone(&preview);
    let Window_weak = Window.as_weak();
    Window.global::<ScreenCallbacks>().on_apply_filters(
        move |box_blur: i32, gaussian_blur: f32, sharpening: f32| {
            //low res preview
            let mut processed: image::ImageBuffer<image::Rgb<u8>, Vec<u8>>;

            let (rw, _) = prev_weak.lock().unwrap().dimensions();
            let mut lr = low_res_weak.lock().unwrap();
            let (lrw, _) = lr.dimensions();

            if sharpening > 0.0 {
                processed = img_processing::sharpen(lr.deref(), sharpening / 2f32 , 3);
            } else {
                processed = lr.deref().clone();
            }

            if box_blur > 3 {
                processed = FastBoxBlur(&processed, box_blur as u32 * lrw / rw);
            }

            if gaussian_blur > 0.0 {
                processed = FastGaussianBlur(&processed, gaussian_blur * lrw as f32 / rw as f32, 5);
            }

            *lr = processed.clone();

            Window_weak
                .upgrade_in_event_loop(move |Window: LVIE| {
                    let pix_buf = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(
                        &processed,
                        processed.width(),
                        processed.height(),
                    );
                    Window.set_image(Image::from_rgb8(pix_buf));

                    /* no longer needed                     
                    Window.set_AlertBoxType(AlertType::Warning);
                    Window.set_AlertText("Low Res preview".into());*/
                    
                    let ww = Window.as_weak();
                    thread::spawn(move || {
                        let path = _create_svg_path(&processed);
                        ww.upgrade_in_event_loop(move |window| {
                            window.set_svg_path(path.into());
                        })
                        .expect("Failed to run in event loop");
                    });
                })
                .expect("Failed to call event loop");
            
            // start computing the full resolution image
            let _w_w = Window_weak.clone();
            let _p_w = prev_weak.clone();
            thread::spawn(move || {
                // full res
                let mut _prev = _p_w.lock().unwrap();

                let mut processed: image::ImageBuffer<image::Rgb<u8>, Vec<u8>>;

                if box_blur > 3 {
                    processed = FastBoxBlur(_prev.deref(), box_blur as u32);
                } else {
                    processed = _prev.clone();
                }

                if sharpening > 0.0 {
                    processed = img_processing::sharpen(&processed, sharpening / 2f32, 5);
                }

                *_prev = processed.clone();

                let ww = _w_w.clone();
                thread::spawn(move || {
                    let path = _create_svg_path(&processed);
                    ww.upgrade_in_event_loop(move |window| {
                        window.set_svg_path(path.into());
                    })
                    .expect("Failed to run in event loop");
                });

                /* there's no needing to load the full resolution image into the UI because the result
                   seen by the user is the same as the low resolution preview!!

                _w_w.upgrade_in_event_loop(move |Window: LVIE| {
                    let pix_buf = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(
                        &processed,
                        processed.width(),
                        processed.height(),
                    );
                    Window.set_image(Image::from_rgb8(pix_buf));
                    Window.set_AlertBoxType(AlertType::Null);
                    let ww = Window.as_weak();
                    thread::spawn(move || {
                        let path = _create_svg_path(&processed);
                        ww.upgrade_in_event_loop(move |window| {
                            window.set_svg_path(path.into());
                        })
                        .expect("Failed to run in event loop");
                    });
                })
                .expect("Failed to call event loop");
                println!("All done");*/
            });
        },
    );

    //set_Alert_Message
    let Window_weak = Window.as_weak();
    Window.global::<ScreenCallbacks>().on_set_Warning_Message(
        move |message: slint::SharedString| {
            let ui = Window_weak.unwrap();
            ui.set_AlertBoxType(AlertType::Warning);
            ui.set_AlertText(message);
        },
    );

    //save
    let img_weak = Arc::clone(&full_res_processed);
    Window
        .global::<ScreenCallbacks>()
        .on_save_file(move |path: SharedString| {
            img_weak
                .lock()
                .unwrap()
                .deref()
                .save(path.as_str())
                .expect("Failed to save file");
        });

    // startup procedure
    let l_weak: Weak<LVIE> = Window.as_weak();

    if WINIT_BACKEND {
        thread::Builder::new()
            .name("waiter".to_string())
            .spawn(move || {
                thread::sleep(time::Duration::from_millis(100));
                l_weak
                    .upgrade_in_event_loop(move |handle| {
                        maximize_ui(handle);
                    })
                    .expect("Failed to call from the main thread");
            })
            .expect("Failed to spawn thread");
    }

    let _ = Window.show();
    slint::run_event_loop().expect("Cannnot run the evnt loop due to an error!");
    let _ = Window.hide();
}
