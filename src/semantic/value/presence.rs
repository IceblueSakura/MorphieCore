//! Explicit presence for fields whose empty, null and absent forms are observable.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Presence<T> {
    #[default]
    Absent,
    Null,
    Value(T),
}
impl<T: serde::Serialize> serde::Serialize for Presence<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Absent | Self::Null => s.serialize_none(),
            Self::Value(v) => v.serialize(s),
        }
    }
}
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Presence<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match Option::<T>::deserialize(d)? {
            None => Self::Null,
            Some(v) => Self::Value(v),
        })
    }
}
impl<T> Presence<T> {
    pub fn value(&self) -> Option<&T> {
        if let Self::Value(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }
}
