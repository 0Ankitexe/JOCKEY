vocabulary!(Platform { Windows => "windows", Ubuntu => "ubuntu" });
vocabulary!(Privilege { User => "user", Elevated => "elevated", LabOnly => "lab_only" });
vocabulary!(Capability {
    System => "system", Processes => "processes", Network => "network", Files => "files",
    Events => "events", Persistence => "persistence", Drivers => "drivers", Indicators => "indicators",
});

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_is_closed_and_ordered() {
        assert_eq!(Platform::from_name("linux"), None);
        assert_eq!(Capability::from_name("execute"), None);
        assert_eq!(Privilege::from_name("root"), None);
        assert!(Privilege::User < Privilege::Elevated && Privilege::Elevated < Privilege::LabOnly);
    }
}
