//! Archive directories (C++ src/core/Directory.cpp, CDirectory): the list of {offset, size, name}
//! records of an .img/.dir, its reading and its case-insensitive lookup. The object stays C++
//! (others read numEntries); its methods are here. Identical to the original
//! (tests/golden/directory.txt).

/// CDirectory::DirectoryInfo.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DirectoryInfo {
    pub offset: u32,
    pub size: u32,
    pub name: [u8; 24],
}

/// CDirectory's data members.
#[repr(C)]
#[derive(Debug)]
pub struct Directory {
    pub entries: *mut DirectoryInfo,
    pub max_entries: i32,
    pub num_entries: i32,
}

const _: () = assert!(std::mem::size_of::<DirectoryInfo>() == 32 && std::mem::size_of::<Directory>() == 16);

/// A record's name as the C string it is (up to its NUL; all 24 bytes when there's none).
fn c_name(n: &[u8]) -> &[u8] {
    &n[..n.iter().position(|&c| c == 0).unwrap_or(n.len())]
}

/// CGeneral::faststricmp(a, b) == 0: equal ignoring ASCII case.
pub fn same_name(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_ascii_uppercase() == y.to_ascii_uppercase())
}

impl Directory {
    fn items(&self) -> &[DirectoryInfo] {
        if self.entries.is_null() || self.num_entries <= 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.entries, self.num_entries as usize) }
        }
    }

    /// FindItem: the first record with the name.
    pub fn find(&self, name: &[u8]) -> Option<(u32, u32)> {
        self.items().iter().find(|e| same_name(c_name(&e.name), name)).map(|e| (e.offset, e.size))
    }

    /// AddItem (FIX_BUGS: a name already there isn't added again). A full directory drops the
    /// record (the original asserts, then writes past the array).
    pub fn add(&mut self, info: &DirectoryInfo) {
        if self.find(c_name(&info.name)).is_some() || self.num_entries >= self.max_entries {
            return;
        }
        unsafe { *self.entries.add(self.num_entries as usize) = *info };
        self.num_entries += 1;
    }

    /// ReadDirFile's loop over the file's bytes: one record per 32 bytes; a short last read still
    /// adds a record (its first bytes over the previous record's, as the original's buffer).
    pub fn add_file(&mut self, data: &[u8]) {
        let mut rec = [0u8; 32];
        for chunk in data.chunks(32) {
            rec[..chunk.len()].copy_from_slice(chunk);
            let info: DirectoryInfo = unsafe { std::mem::transmute(rec) };
            self.add(&info);
        }
    }
}

/// # Safety
/// `dir` is a valid CDirectory; `data` holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_dir_add_file(dir: *mut Directory, data: *const u8, len: usize) {
    unsafe { (*dir).add_file(if data.is_null() { &[] } else { std::slice::from_raw_parts(data, len) }) }
}

/// # Safety
/// `dir` is a valid CDirectory; `info` a valid record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_dir_add(dir: *mut Directory, info: *const DirectoryInfo) {
    unsafe { (*dir).add(&*info) }
}

/// # Safety
/// `dir` is a valid CDirectory; `name` a C string; `offset`/`size` valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_dir_find(dir: *const Directory, name: *const std::ffi::c_char, offset: *mut u32, size: *mut u32) -> bool {
    unsafe {
        match (*dir).find(std::ffi::CStr::from_ptr(name).to_bytes()) {
            Some((o, s)) => {
                *offset = o;
                *size = s;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod golden {
    //! tests/golden/directory.txt: the original CDirectory (ReadDirFile, AddItem with an img id,
    //! FindItem) on synthetic .dir files in tests/golden/directory/: the entries after reading and
    //! the answers to lookups.
    use super::*;

    #[test]
    fn matches_cpp_golden() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let golden = std::fs::read_to_string(root.join("tests/golden/directory.txt")).unwrap();
        let mut checked = 0;
        for line in golden.lines() {
            let (head, want) = line.split_once(" => ").unwrap();
            let mut words = head.split(' ');
            let file = words.next().unwrap();
            let max: i32 = words.next().unwrap().parse().unwrap();
            let queries: Vec<&str> = words.collect();
            let raw = std::fs::read(root.join("tests/golden/directory").join(file)).unwrap();
            let mut store = vec![DirectoryInfo::default(); max as usize];
            let mut dir = Directory { entries: store.as_mut_ptr(), max_entries: max, num_entries: 0 };
            dir.add_file(&raw);
            // AddItem(info, imgId): a record with the image id in the offset's top byte.
            let mut extra = DirectoryInfo { offset: 0x1234, size: 7, name: [0; 24] };
            extra.name[..9].copy_from_slice(b"extra.txd");
            extra.offset |= 3 << 24;
            dir.add(&extra);
            let mut got = format!("{}", dir.num_entries);
            for e in dir.items() {
                got += &format!(" {:08x}:{:08x}", e.offset, e.size);
            }
            for q in queries {
                got += &match dir.find(q.as_bytes()) {
                    Some((o, s)) => format!(" {q}={o:08x}:{s:08x}"),
                    None => format!(" {q}=-"),
                };
            }
            assert_eq!(got, want, "{file}");
            checked += 1;
        }
        assert!(checked >= 8);
    }
}
