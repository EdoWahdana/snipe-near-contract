use crate::*;

#[near_bindgen]
impl Contract {
    // Owner private methods

    /// @allow ["::admins", "::owner"]
    pub fn update_uri(&mut self, uri: String) -> bool {
        self.assert_owner_or_admin();
        let mut metadata = self.metadata.get().unwrap();
        log!("New URI: {}", &uri);
        metadata.base_uri = Some(uri);
        self.metadata.set(&metadata);
        true
    }
}
