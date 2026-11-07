//! A bounded overview of projected, loaded history. Bin counts cap paint work independently of history depth.
use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use super::ROW_PITCH;
use crate::repo::{RepoTab, saved_views};
use crate::theme::Tokens;
const BINS: usize = 512;
const LANES: usize = 24;
#[derive(Clone, Default)]
struct Bin { lanes: u32, head: bool, tag: bool, hit: bool, gap: bool }
pub(in crate::repo) struct State {
    pub dirty: bool,
    bins: Vec<Bin>,
    rows: usize,
    pub offset: Option<f32>,
    drag_anchor: f32,
}
impl Default for State {
    fn default() -> Self { Self { dirty: true, bins: Vec::new(), rows: 0, offset: None, drag_anchor: 0.0 } }
}
fn bin_for(row: usize, rows: usize, bins: usize) -> usize { (row.saturating_mul(bins) / rows.max(1)).min(bins.saturating_sub(1)) }
fn offset_at(fraction: f32, rows: usize, viewport: f32, anchor: f32) -> f32 {
    (fraction.clamp(0.0,1.0) * rows as f32 * ROW_PITCH - anchor).clamp(0.0,(rows as f32 * ROW_PITCH - viewport).max(0.0))
}
impl RepoTab {
    fn prepare_minimap(&mut self) {
        if !self.history_minimap.dirty { return; }
        let filtered = self.saved_views.active();
        let rows = if filtered { self.saved_views.len() } else { self.commits.len() };
        let mut bins = vec![Bin::default(); rows.min(BINS)];
        for projected in 0..rows {
            let (raw, gap) = if filtered {
                match self.saved_views.row(projected) {
                    Some(saved_views::Row::Commit(raw)) => (*raw,false),
                    Some(saved_views::Row::Gap { range, .. }) => (range.start,true),
                    None => continue,
                }
            } else { (projected,false) };
            let Some(c) = self.commits.get(raw) else { continue; };
            let Some(g) = self.graph.rows.get(raw) else { continue; };
            let bin = &mut bins[bin_for(projected,rows,rows.min(BINS))];
            for col in std::iter::once(&g.col).chain(&g.through).chain(&g.incoming).chain(&g.outgoing) {
                if *col < LANES { bin.lanes |= 1 << col; }
            }
            bin.gap |= gap;
            bin.head |= c.hash == self.refs.head;
            bin.tag |= c.refs.iter().any(|r| r.starts_with("tag: ") || r.starts_with("refs/tags/"));
            bin.hit |= self.search.matches.contains(&c.hash);
            if gap && let Some(saved_views::Row::Gap { range, .. }) = self.saved_views.row(projected) {
                // A collapsed span can contain a tag, HEAD or a search hit; keep its marker at the connector.
                for c in &self.commits[range.clone()] {
                    bin.head |= c.hash == self.refs.head;
                    bin.tag |= c.refs.iter().any(|r| r.starts_with("tag: ") || r.starts_with("refs/tags/"));
                    bin.hit |= self.search.matches.contains(&c.hash);
                }
            }
        }
        self.history_minimap.bins = bins;
        self.history_minimap.rows = rows;
        self.history_minimap.dirty = false;
    }
    pub(super) fn history_minimap(&mut self, ui: &mut Ui, rect: Rect, offset: f32, viewport: f32) {
        self.prepare_minimap();
        let t = Tokens::get(ui.ctx());
        let response = ui.interact(rect, ui.id().with("history-minimap"), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 3.0, t.dock);
        p.text(pos2(rect.center().x,rect.top()+3.0),Align2::CENTER_TOP,"History",FontId::proportional(10.0),t.text_dim);
        let map = Rect::from_min_max(pos2(rect.left()+4.0,rect.top()+19.0),pos2(rect.right()-4.0,rect.bottom()-20.0));
        if map.height() <= 0.0 || self.history_minimap.rows == 0 { return; }
        let n = self.history_minimap.bins.len();
        let shown = self.graph.width.clamp(1,LANES);
        for (i, bin) in self.history_minimap.bins.iter().enumerate() {
            let y = map.top() + (i as f32+0.5)/n as f32 * map.height();
            let height = (map.height()/n as f32).max(1.0);
            for lane in 0..shown {
                if bin.lanes & (1 << lane) != 0 {
                    let x = map.left()+3.0+lane as f32/shown as f32*(map.width()-13.0);
                    p.line_segment([pos2(x,y),pos2(x,y+height)],Stroke::new(1.2, if bin.gap { t.text_faint } else { t.text_dim }));
                }
            }
            if bin.tag { p.circle_filled(pos2(map.right()-5.0,y),2.0,t.success); }
            if bin.hit { p.line_segment([pos2(map.left(),y),pos2(map.left()+4.0,y)],Stroke::new(2.0,t.warning)); }
            if bin.head { p.line_segment([pos2(map.right()-8.0,y),pos2(map.right(),y)],Stroke::new(2.0,t.accent)); }
        }
        let full_h = self.history_minimap.rows as f32 * ROW_PITCH;
        let top = (offset/full_h).clamp(0.0,1.0);
        let bottom = ((offset+viewport)/full_h).clamp(top,1.0);
        let visible = Rect::from_min_max(pos2(map.left(),map.top()+top*map.height()),pos2(map.right(),(map.top()+bottom*map.height()).max(map.top()+top*map.height()+3.0)));
        p.rect_stroke(visible,1.0,Stroke::new(1.5,t.accent),StrokeKind::Inside);
        let label = if self.has_more { "Older unloaded ↓" } else { "History ends" };
        p.line_segment([pos2(map.left(),map.bottom()),map.right_bottom()],Stroke::new(1.0,t.text_faint));
        p.text(pos2(rect.center().x,rect.bottom()-3.0),Align2::CENTER_BOTTOM,label,FontId::proportional(8.0),t.text_dim);
        if response.drag_started() && let Some(pointer) = response.interact_pointer_pos() {
            self.history_minimap.drag_anchor = if visible.contains(pointer) { ((pointer.y-map.top())/map.height()*full_h-offset).clamp(0.0,viewport) } else { viewport/2.0 };
        }
        if (response.dragged() || response.clicked()) && let Some(pointer) = response.interact_pointer_pos() {
            let anchor = if response.clicked() { viewport/2.0 } else { self.history_minimap.drag_anchor };
            self.history_minimap.offset = Some(offset_at((pointer.y-map.top())/map.height(),self.history_minimap.rows,viewport,anchor));
            response.request_focus();
            ui.ctx().request_repaint();
        }
        if response.has_focus() {
            let delta = ui.input(|i| if i.key_pressed(egui::Key::ArrowDown) { ROW_PITCH } else if i.key_pressed(egui::Key::ArrowUp) { -ROW_PITCH } else if i.key_pressed(egui::Key::PageDown) { viewport } else if i.key_pressed(egui::Key::PageUp) { -viewport } else { 0.0 });
            if delta != 0.0 { self.history_minimap.offset = Some((offset+delta).clamp(0.0,(full_h-viewport).max(0.0))); }
        }
        response.on_hover_text(format!("{} projected rows of loaded history. Drag to scroll; focused: arrows or Page Up/Down. Blue: HEAD · green: tags · yellow: search hits · grey: collapsed context. {}",self.history_minimap.rows,if self.has_more {"Older history is not loaded; scrolling to the boundary loads another page."} else {"All available history is loaded."}));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_and_drag_anchor_respect_loaded_extent() {
        assert_eq!(bin_for(99_999,100_000,BINS),BINS-1);
        assert_eq!(offset_at(0.0,1000,300.0,150.0),0.0);
        assert_eq!(offset_at(1.0,1000,300.0,150.0),29_700.0);
        assert_eq!(offset_at(0.5,1000,300.0,50.0),14_950.0);
        assert_eq!(offset_at(1.0,2,300.0,150.0),0.0);
    }
}
