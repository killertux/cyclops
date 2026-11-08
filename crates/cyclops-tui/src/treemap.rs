//! Treemaps: rectangles in proportion to weights, kept close to square.

use ratatui::layout::Rect;

/// How many whole steps of `limit` fit in `share` of it, a share being from 0 to 1.
#[must_use]
pub fn steps(share: f64, limit: u16) -> u16 {
    let target = share.clamp(0.0, 1.0) * f64::from(limit);
    (0..=limit)
        .rev()
        .find(|step| f64::from(*step) <= target + 0.5)
        .unwrap_or(0)
}

/// Splits an area into one rectangle for each weight, keeping each close to square: a
/// squarified treemap. A cell is about twice as tall as it is wide, so widths count
/// double.
#[must_use]
pub fn squarify(weights: &[f64], area: Rect) -> Vec<Rect> {
    let weight = |index: usize| weights.get(index).copied().unwrap_or(0.0);
    let mut placed = vec![Rect::default(); weights.len()];
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by(|a, b| weight(*b).total_cmp(&weight(*a)));
    let mut left = area;
    let mut remaining: f64 = weights.iter().sum();
    let mut index = 0;
    while index < order.len() && left.width > 0 && left.height > 0 && remaining > 0.0 {
        let wide = left.width >= left.height * 2;
        let side = if wide {
            f64::from(left.height) * 2.0
        } else {
            f64::from(left.width)
        };
        let scale = f64::from(left.width) * f64::from(left.height) * 2.0 / remaining;
        let row = best_row(order.get(index..).unwrap_or_default(), &weight, scale, side);
        let row_weight: f64 = row.iter().map(|member| weight(*member)).sum();
        let last = index + row.len() >= order.len();
        left = place_row(
            &mut placed,
            &row,
            &weight,
            left,
            row_weight / remaining,
            wide,
            last,
        );
        remaining -= row_weight;
        index += row.len().max(1);
    }
    placed
}

/// The members of the next row: as many as keep the worst ratio of sides from growing.
fn best_row(next: &[usize], weight: &dyn Fn(usize) -> f64, scale: f64, side: f64) -> Vec<usize> {
    let mut row: Vec<usize> = Vec::new();
    let mut total = 0.0;
    let mut worst = f64::INFINITY;
    for member in next {
        let trial = total + weight(*member);
        let thickness = trial * scale / side;
        let ratio = row
            .iter()
            .chain(std::iter::once(member))
            .map(|one| {
                let length = weight(*one) * scale / thickness;
                (length / thickness).max(thickness / length)
            })
            .fold(0.0, f64::max);
        if !row.is_empty() && ratio > worst {
            break;
        }
        worst = ratio;
        row.push(*member);
        total = trial;
    }
    row
}

/// Places one row along the short side of what is left, and returns what is left after.
fn place_row(
    placed: &mut [Rect],
    row: &[usize],
    weight: &dyn Fn(usize) -> f64,
    left: Rect,
    share: f64,
    wide: bool,
    last: bool,
) -> Rect {
    let total: f64 = row.iter().map(|member| weight(*member)).sum();
    let (across, along) = if wide {
        (left.width, left.height)
    } else {
        (left.height, left.width)
    };
    let thickness = if last {
        across
    } else {
        steps(share, across).max(1)
    };
    let mut offset = 0;
    for (position, member) in row.iter().enumerate() {
        let length = if position + 1 == row.len() {
            along - offset
        } else {
            steps(weight(*member) / total.max(f64::MIN_POSITIVE), along).max(1)
        };
        let rect = if wide {
            Rect::new(left.x, left.y + offset, thickness, length)
        } else {
            Rect::new(left.x + offset, left.y, length, thickness)
        };
        if let Some(slot) = placed.get_mut(*member) {
            *slot = rect;
        }
        offset += length;
    }
    if wide {
        Rect::new(
            left.x + thickness,
            left.y,
            left.width.saturating_sub(thickness),
            left.height,
        )
    } else {
        Rect::new(
            left.x,
            left.y + thickness,
            left.width,
            left.height.saturating_sub(thickness),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(rect: Rect) -> u32 {
        u32::from(rect.width) * u32::from(rect.height)
    }

    #[test]
    fn a_treemap_fills_the_area_and_larger_weighs_more() {
        let area = Rect::new(0, 0, 60, 20);
        let tiles = squarify(&[10.0, 40.0, 20.0, 30.0], area);
        let total: u32 = tiles.iter().map(|tile| cells(*tile)).sum();
        assert_eq!(total, cells(area));
        let size = |index: usize| tiles.get(index).map_or(0, |tile| cells(*tile));
        assert!(size(1) > size(3) && size(3) > size(2) && size(2) > size(0));
        for tile in &tiles {
            assert!(
                tile.x + tile.width <= 60 && tile.y + tile.height <= 20,
                "{tile:?}"
            );
        }
    }

    #[test]
    fn nothing_to_lay_out_is_nothing_drawn() {
        assert_eq!(squarify(&[], Rect::new(0, 0, 10, 10)), Vec::new());
    }
}
