# Worker results

Workers may write one uniquely named result record here and return it to the coordinator.

The coordinator decides when a result is merged into shared state. Result records must not contain credentials or claim verification that was not performed.
