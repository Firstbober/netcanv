use std::ffi::CString;
use std::os::raw::c_char;

extern {
   fn print(content: *mut c_char);
}

#[no_mangle]
pub extern fn greet() {
   unsafe {
      print(CString::new("hello").unwrap().into_raw());
   }
}
