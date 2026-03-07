use crate::types::{PageId, TagName};
use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub enum PickerFilter {
    Tag(TagName),
    DateRange(NaiveDate, NaiveDate),
    LinksTo(PageId),
    TaskStatus(bool),
}
