#[cfg(test)]
mod time_series_tests {
    use chrono::{Duration, Utc};
    use std::time::Duration as StdDuration;
    
    use crate::metrics::timeseries::{TimeSeries, TimeSeriesPoint, AsRef};

    #[test]
    fn test_time_series_creation() {
        let ts = TimeSeries::<f64>::new("test_series".to_string(), 10);
        assert_eq!(ts.name, "test_series");
        assert_eq!(ts.max_points, 10);
        assert!(ts.data.is_empty());
    }

    #[test]
    fn test_add_point() {
        let mut ts = TimeSeries::<f64>::new("test_series".to_string(), 3);
        let now = Utc::now();
        
        // Add three points
        ts.add_point(now, 10.0);
        ts.add_point(now + Duration::seconds(1), 20.0);
        ts.add_point(now + Duration::seconds(2), 30.0);
        
        assert_eq!(ts.data.len(), 3);
        assert_eq!(ts.data[0].value, 10.0);
        assert_eq!(ts.data[1].value, 20.0);
        assert_eq!(ts.data[2].value, 30.0);
        
        // Add one more point, should push out the oldest
        ts.add_point(now + Duration::seconds(3), 40.0);
        
        assert_eq!(ts.data.len(), 3);
        assert_eq!(ts.data[0].value, 20.0);
        assert_eq!(ts.data[1].value, 30.0);
        assert_eq!(ts.data[2].value, 40.0);
    }

    #[test]
    fn test_get_latest() {
        let mut ts = TimeSeries::<f64>::new("test_series".to_string(), 3);
        
        // Empty series should return None
        assert!(ts.get_latest().is_none());
        
        // Add a point and check it's returned
        let now = Utc::now();
        ts.add_point(now, 10.0);
        
        let latest = ts.get_latest().unwrap();
        assert_eq!(latest.value, 10.0);
        
        // Add another point and check it becomes latest
        ts.add_point(now + Duration::seconds(1), 20.0);
        let latest = ts.get_latest().unwrap();
        assert_eq!(latest.value, 20.0);
    }

    #[test]
    fn test_get_range_from_now() {
        let mut ts = TimeSeries::<f64>::new("test_series".to_string(), 5);
        let now = Utc::now();
        
        // Add points at different times
        ts.add_point(now - Duration::seconds(60), 10.0);
        ts.add_point(now - Duration::seconds(30), 20.0);
        ts.add_point(now - Duration::seconds(10), 30.0);
        
        // Get points from last 20 seconds
        let recent_points = ts.get_range_from_now(StdDuration::from_secs(20));
        assert_eq!(recent_points.len(), 1);
        assert_eq!(recent_points[0].value, 30.0);
        
        // Get points from last 40 seconds
        let recent_points = ts.get_range_from_now(StdDuration::from_secs(40));
        assert_eq!(recent_points.len(), 2);
        assert_eq!(recent_points[0].value, 20.0);
        assert_eq!(recent_points[1].value, 30.0);
    }

    #[test]
    fn test_get_range() {
        let mut ts = TimeSeries::<f64>::new("test_series".to_string(), 5);
        let now = Utc::now();
        
        // Add points at different times
        ts.add_point(now - Duration::seconds(60), 10.0);
        ts.add_point(now - Duration::seconds(30), 20.0);
        ts.add_point(now - Duration::seconds(10), 30.0);
        
        // Get points in a specific time range
        let range_points = ts.get_range(
            now - Duration::seconds(40),
            now - Duration::seconds(5)
        );
        
        assert_eq!(range_points.len(), 2);
        assert_eq!(range_points[0].value, 20.0);
        assert_eq!(range_points[1].value, 30.0);
    }

    #[test]
    fn test_prune_older_than() {
        let mut ts = TimeSeries::<f64>::new("test_series".to_string(), 5);
        let now = Utc::now();
        
        // Add points at different times
        ts.add_point(now - Duration::seconds(60), 10.0);
        ts.add_point(now - Duration::seconds(30), 20.0);
        ts.add_point(now - Duration::seconds(10), 30.0);
        
        assert_eq!(ts.data.len(), 3);
        
        // Prune points older than 20 seconds ago
        ts.prune_older_than(now - Duration::seconds(20));
        
        assert_eq!(ts.data.len(), 1);
        assert_eq!(ts.data[0].value, 30.0);
    }

    #[test]
    fn test_min_max() {
        // Create a helper struct that implements AsRef
        struct TestValue(f64);
        
        impl AsRef<f64> for TestValue {
            fn as_ref(&self) -> &f64 {
                &self.0
            }
        }
        
        let mut ts = TimeSeries::<TestValue>::new("test_series".to_string(), 5);
        
        // Empty series should return None
        assert!(ts.min_max().is_none());
        
        let now = Utc::now();
        
        // Add points with different values
        ts.add_point(now, TestValue(30.0));
        ts.add_point(now + Duration::seconds(1), TestValue(10.0));
        ts.add_point(now + Duration::seconds(2), TestValue(50.0));
        ts.add_point(now + Duration::seconds(3), TestValue(20.0));
        
        // Check min and max values
        let (min, max) = ts.min_max().unwrap();
        assert_eq!(min, 10.0);
        assert_eq!(max, 50.0);
    }

    #[test]
    fn test_with_metadata() {
        let ts = TimeSeries::<f64>::new("test_series".to_string(), 10)
            .with_metadata("unit", "percent")
            .with_metadata("source", "test");
            
        assert_eq!(ts.metadata.len(), 2);
        assert_eq!(ts.metadata.get("unit"), Some(&"percent".to_string()));
        assert_eq!(ts.metadata.get("source"), Some(&"test".to_string()));
    }
}