pub struct UnitTag;

impl UnitTag {
    /// Convert a unit index/recycle pair to a unit tag value.
    pub fn from_parts(unit_tag_index: i128, unit_tag_recycle: i128) -> i128 {
        (unit_tag_index << 18) + unit_tag_recycle
    }

    /// Extract the unit index from a unit tag.
    pub fn index(unit_tag: i128) -> i128 {
        (unit_tag >> 18) & 0x00003fff
    }

    /// Extract the unit recycle value from a unit tag.
    pub fn recycle(unit_tag: i128) -> i128 {
        unit_tag & 0x0003ffff
    }
}
