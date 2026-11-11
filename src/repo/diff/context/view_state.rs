use super::*;

impl crate::repo::DiffView {
    /// Opens gap `g` of the diff shown as `how` says, keeping what points at its rows (lines
    /// picked, text selected, the line to scroll to) on the same lines; the lines opened stay
    /// open when the diff is read again.
    pub(in crate::repo) fn unfold(&mut self, g: usize, how: Unfold) {
        let Some(Ok(data)) = &mut self.data else {
            return;
        };
        let shifts = data.unfold(g, how);
        let mut opened = Vec::new();
        for shift in &shifts {
            if shift.count > 0 {
                let added = shift.at..shift.at + shift.count as usize;
                opened.extend(data.lines[added].iter().filter_map(|l| l.new));
            }
        }
        self.revealed.extend(opened);
        for shift in shifts {
            self.follow_rows(shift);
        }
        self.recompare_moved();
    }

    /// Opens again the lines opened before the diff was read again.
    pub(in crate::repo) fn reopen(&mut self) {
        super::super::presentation::reset_geometry(self);
        let Some(Ok(data)) = &mut self.data else {
            return;
        };
        let Some(ctx) = &data.context else { return };
        let mut wanted = Vec::new();
        for (g, gap) in ctx.gaps.iter().enumerate() {
            let below = (gap.first..=gap.last)
                .rev()
                .take_while(|n| self.revealed.contains(n))
                .count() as u32;
            let above = (gap.first..=gap.last.saturating_sub(below))
                .take_while(|n| self.revealed.contains(n))
                .count() as u32;
            if !gap.trailing {
                wanted.push((g, Unfold::Up(below)));
            }
            if gap.opens_down(g) {
                let above = if gap.trailing {
                    above.max(below)
                } else {
                    above
                };
                wanted.push((g, Unfold::Down(above)));
            }
        }
        let shifts: Vec<Shift> = wanted
            .into_iter()
            .flat_map(|(g, how)| data.unfold(g, how))
            .collect();
        for shift in shifts {
            self.follow_rows(shift);
        }
    }

    /// The moved block shown, compared again on the rows as they are now.
    pub(super) fn recompare_moved(&mut self) {
        if let (Some(Ok(data)), Some(open)) = (&self.data, &self.moved) {
            self.moved = super::super::moved_view::Opened::new(data, open.block);
        }
    }

    /// Moves what points at rows along with `shift`.
    pub(super) fn follow_rows(&mut self, shift: Shift) {
        super::super::presentation::follow_rows(self, shift);
    }
}
