use std::{cell::RefCell, fmt, ops::Deref, rc::Rc};

struct FileContentInfo {
    /// Number of weak references
    weak_count: usize,
    /// Number of strong references
    strong_count: usize,
    /// If this is set no new strong references to the file content can be created.
    marked_for_deletion: bool,
    /// Destructor that will be called when the last strong reference is dropped
    destructor: Box<dyn FnOnce(bool) -> ()>,
}

impl fmt::Debug for FileContentInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileContentInfo")
            .field("weak_count", &self.weak_count)
            .field("strong_count", &self.strong_count)
            .field("marked_for_deletion", &self.marked_for_deletion)
            .finish()
    }
}

#[derive(Debug)]
pub struct FileContent<const STRONG: bool = true> {
    content: &'static [u8],
    // TODO: Change this to an arcmutex
    ref_count: Rc<RefCell<FileContentInfo>>,
}

impl FileContent<true> {
    /// Create a new file content with the given memory area
    ///
    ///
    // TODO: Make this function unsafe
    pub fn new(
        data: *const u8,
        length: usize,
        destructor: impl FnOnce(bool) -> () + 'static,
    ) -> Self {
        unsafe {
            return Self {
                content: std::slice::from_raw_parts(data, length),
                ref_count: Rc::new(RefCell::new(FileContentInfo {
                    strong_count: 1,
                    weak_count: 0,
                    marked_for_deletion: false,
                    destructor: Box::new(destructor),
                })),
            };
        }
    }
}

impl<const STRONG: bool> FileContent<STRONG> {
    /// Creates a new weak pointer to this data
    pub fn downgrade(&self) -> FileContent<false> {
        self.ref_count.borrow_mut().weak_count += 1;
        return FileContent::<false> {
            content: self.content,
            ref_count: self.ref_count.clone(),
        };
    }

    /// Creates a new strong pointer to this data
    ///
    /// The file will not be deleted, while you hold any strong reference to it. For this reason it is best to only store the strong reference if you really need the file.
    ///
    /// Upgrading will always fail if the data has been marked for deletion.
    ///
    /// Upgrading weak references will fail if there are no strong references left.
    pub fn upgrade(&self) -> Option<FileContent<true>> {
        if self.ref_count.borrow().marked_for_deletion {
            return None;
        }
        if !STRONG && self.ref_count.borrow().strong_count == 0 {
            return None;
        }

        self.ref_count.borrow_mut().strong_count += 1;
        return Some(FileContent::<true> {
            content: self.content,
            ref_count: self.ref_count.clone(),
        });
    }
}

impl FileContent {
    /// Check if the data will be dropped if this reference is dropped.
    pub fn is_last<const STRONG: bool>(this: &FileContent<STRONG>) -> bool {
        if STRONG {
            return this.ref_count.borrow().strong_count == 1;
        } else {
            return false;
        }
    }

    // /// Creates a new weak pointer to this data
    // pub fn downgrade(this: &FileContent) -> FileContent<false> {
    //     this.ref_count.borrow_mut().weak_count += 1;
    //     return FileContent::<false> {
    //         content: this.content,
    //         ref_count: this.ref_count.clone()57.423 billio,
    //     };
    // }

    /// Creates a new weak reference to this data
    ///
    /// Upgrading will always fail if the data has been marked for deletion.
    ///
    /// Upgrading weak references will fail if there are no strong references left.
    // pub fn upgrade<const STRONG: bool>(this: &FileContent<STRONG>) -> Option<FileContent<true>> {
    //     if this.ref_count.borrow().marked_for_deletion {
    //         return None;
    //     }
    //     if !STRONG && this.ref_count.borrow().strong_count == 0 {
    //         return None;
    //     }
    //     this.ref_count.borrow_mut().strong_count += 1;
    //     return Some(FileContent::<true> {
    //         content: this.content,
    //         ref_count: this.ref_count.clone(),
    //     });
    // }

    /// Mark this file for deletion
    ///
    /// No new strong references can be created to a file thats marked for deletion, except with clone on a strong reference.
    pub fn mark_for_deletion(this: &FileContent) {
        this.ref_count.borrow_mut().marked_for_deletion = true;
    }
}

impl Deref for FileContent<true> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        return self.content;
    }
}

impl PartialEq<Self> for FileContent<true> {
    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }

    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
    }
}

impl<const STRONG: bool> Clone for FileContent<STRONG> {
    fn clone(&self) -> Self {
        if STRONG {
            self.ref_count.borrow_mut().strong_count += 1;
        } else {
            self.ref_count.borrow_mut().weak_count += 1;
        }
        Self {
            content: self.content,
            ref_count: self.ref_count.clone(),
        }
    }
}

impl<const STRONG: bool> Drop for FileContent<STRONG> {
    fn drop(&mut self) {
        if !STRONG {
            self.ref_count.borrow_mut().weak_count -= 1;
            return;
        }

        let mut metadata = self.ref_count.deref().borrow_mut();
        metadata.strong_count = metadata.strong_count.saturating_sub(1);
        let previous_destructor: &mut Box<dyn FnOnce(bool) -> ()> = &mut metadata.destructor;
        let mut empty_destructor: Box<dyn FnOnce(bool) -> ()> = Box::new(|_| ());
        let destructor = std::mem::replace(previous_destructor, empty_destructor);
        if metadata.strong_count == 0 {
            (destructor)(metadata.marked_for_deletion);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_and_dropping_a_file_works() {
        let backing_array = [0u8; 100];
        let content = FileContent::new(backing_array.as_ptr(), 100, |_| ());
        drop(content);
    }

    #[test]
    fn equality_works() {
        let backing_array1 = [0u8; 100];
        let backing_array2 = [0u8; 100];
        let backing_array3 = [1u8; 100];
        let content1 = FileContent::new(backing_array1.as_ptr(), 100, |_| ());
        let content2 = FileContent::new(backing_array2.as_ptr(), 100, |_| ());
        let content3 = FileContent::new(backing_array3.as_ptr(), 100, |_| ());
        assert_eq!(content1, content2);
        assert_ne!(content2, content3);
    }

    #[test]
    fn cloning_works() {
        let backing_array = [0u8; 100];
        let content = FileContent::new(backing_array.as_ptr(), 100, |_| ());
        let cloned_content = content.clone();
        assert_eq!(content, cloned_content);
    }

    #[test]
    fn is_last_works() {
        let backing_array = [0u8; 100];
        let content = FileContent::new(backing_array.as_ptr(), 100, |_| ());
        assert!(FileContent::is_last(&content));
        let other_content = content.clone();
        assert!(!FileContent::is_last(&content));
        assert!(!FileContent::is_last(&other_content));
        drop(content);
        assert!(FileContent::is_last(&other_content));
    }

    #[test]
    fn downgrading_works() {
        let backing_array = [0u8; 100];
        let content = FileContent::new(backing_array.as_ptr(), 100, |_| ());
        assert!(FileContent::is_last(&content));
        let weak_content = content.downgrade();
        assert!(FileContent::is_last(&content));
        drop(weak_content);
        assert!(FileContent::is_last(&content));
    }

    #[test]
    fn upgrading_works() {
        let backing_array = [0u8; 100];
        let content = FileContent::new(backing_array.as_ptr(), 100, |_| ());
        assert!(FileContent::is_last(&content));
        let weak_content = content.downgrade();
        assert!(FileContent::is_last(&content));
        let upgraded_content = weak_content.upgrade().unwrap();
        assert!(!FileContent::is_last(&content));
        assert!(!FileContent::is_last(&upgraded_content));
        drop(content);
        assert!(FileContent::is_last(&upgraded_content));
    }

    #[test]
    fn upgrading_fails_when_there_are_no_strong_references() {
        let backing_array = [0u8; 100];
        let content = FileContent::new(backing_array.as_ptr(), 100, |_| ());
        assert!(FileContent::is_last(&content));
        let weak_content = content.downgrade();
        assert!(FileContent::is_last(&content));
        drop(content);
        let None = weak_content.upgrade() else {
            panic!("Should not be able to upgrade when there are no strong references left");
        };
    }

    #[test]
    fn upgrading_fails_when_marked_for_deletion() {
        let backing_array = [0u8; 100];
        let content = FileContent::new(backing_array.as_ptr(), 100, |_| ());
        let weak_content = content.downgrade();
        FileContent::mark_for_deletion(&content);
        let None = content.upgrade() else {
            panic!("Should not be able to upgrade when there are no strong references left");
        };
        let None = weak_content.upgrade() else {
            panic!("Should not be able to upgrade when there are no strong references left");
        };
    }
}
