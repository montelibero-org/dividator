/// In memory cache that is restored each time 
/// service is restarted. It contains info that 
/// is fetched from other services.
pub struct Cache {

}

impl Cache {
    pub fn new() -> Self {
        Cache { 

        }
    }
}

impl Default for Cache {
    fn default() -> Self {
        Cache::new()
    }
}