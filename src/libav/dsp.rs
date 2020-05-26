pub fn mean(data: &[i32]) -> Option<f32> {
    let sum = data.iter().sum::<i32>() as f32;
    let count = data.len();

    match count {
        i if i > 0 => Some(sum / count as f32),
        _ => None,
    }
}

pub fn std_deviation(data: &[i32]) -> Option<f32> {
    match (mean(data), data.len()) {
        (Some(data_mean), count) if count > 0 => {
            let variance = data
                .iter()
                .map(|&value| {
                    let diff = data_mean - (value as f32);
                    diff * diff
                })
                .sum::<f32>()
                / count as f32;

            Some(variance.sqrt())
        }
        _ => None,
    }
}

pub fn normalize_to_stdev(data: &[i32]) -> Vec<f32> {
    let stdev = match std_deviation(&data) {
        Some(x) => x,
        None => return Vec::new(),
    };

    data.iter().map(|&s| s as f32 / stdev).collect()
}

pub fn covariance(x: &[i32], y: &[i32]) -> f32 {
    let x = normalize_to_stdev(&x);
    let y = normalize_to_stdev(&y);

    let x_mean = x.iter().sum::<f32>() / x.len() as f32;
    let y_mean = y.iter().sum::<f32>() / y.len() as f32;

    let data_len = x.len().min(y.len());
    let covariance = x
        .iter()
        .zip(y.iter())
        .map(|(x_i, y_i)| (x_i - x_mean) * (y_i - y_mean))
        .sum::<f32>()
        / data_len as f32;

    // the value can sometimes be just ever so slightly above 1
    covariance.min(1.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn covariance_test() {
        let left = [
            626568, 609464, 584472, 554376, 525872, 495784, 468344, 430432, 388816, 340976, 304384,
            262848, 234328, 208400, 190840, 173624, 169464, 174472, 191784, 199824, 216792, 224032,
            234848, 228512, 230416, 226376, 224512, 223504, 222344, 217440, 232520, 250272, 279112,
            294528, 299544, 288160, 287368, 276184, 271848, 257376,
        ];
        let right = [
            627024, 610016, 585184, 555376, 527168, 497264, 470064, 432432, 390912, 343376, 307008,
            265888, 237664, 212096, 194720, 177744, 173728, 178976, 196464, 204704, 221792, 229120,
            239872, 233536, 235440, 231392, 229472, 228528, 227264, 222320, 237344, 255264, 284144,
            299648, 304704, 293536, 292832, 281872, 277664, 263440,
        ];

        let covariance = super::covariance(&left, &right);
        assert_eq!(format!("{:.6}", covariance), "0.999977");
    }
}
