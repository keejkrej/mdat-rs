use std::path::Path;

use crate::error::Result;
use crate::input::ImageInfo;
use crate::input::ReaderSession;
use crate::io::write_tiff;
use crate::output::{emit_progress, OutputFormatWriter, ProgressPhase};
use crate::selection::ConvertSelection;

pub struct MdatOutputFormat;

impl OutputFormatWriter for MdatOutputFormat {
    fn name(&self) -> &'static str {
        "mdat"
    }

    fn position_label(&self, p_idx: usize) -> String {
        format!("Pos{p_idx}")
    }

    fn run_convert(
        &self,
        _input_path: &Path,
        output: &Path,
        selection: &ConvertSelection,
        _info: &ImageInfo,
        session: &mut ReaderSession,
        on_progress: &mut Option<&mut dyn crate::output::ProgressCallback>,
    ) -> Result<()> {
        let total = selection.pos_indices.len()
            * selection.time_indices.len()
            * selection.channel_indices.len()
            * selection.z_indices.len();

        emit_progress(
            on_progress,
            ProgressPhase::Start,
            0,
            total,
            format!(
                "Selected {} positions, {} timepoints, {} channels, {} z-slices. Total frames: {total}",
                selection.pos_indices.len(),
                selection.time_indices.len(),
                selection.channel_indices.len(),
                selection.z_indices.len(),
            ),
        );

        std::fs::create_dir_all(output)?;
        let mut done = 0usize;

        for &p_idx in &selection.pos_indices {
            let pos_dir = output.join(format!("Pos{p_idx}"));
            std::fs::create_dir_all(&pos_dir)?;

            // Use original source time indices in filenames (gaps allowed).
            // Downstream tools (e.g. lisca folder source) discover times from
            // names and support non-contiguous series — no time_map renumbering.
            for &t_orig in &selection.time_indices {
                for &c_orig in &selection.channel_indices {
                    for &z_orig in &selection.z_indices {
                        let frame = session.read_frame(p_idx, t_orig, c_orig, z_orig)?;
                        let filename = format!(
                            "img_channel{c_orig:03}_position{p_idx:03}_time{t_orig:09}_z{z_orig:03}.tif"
                        );
                        write_tiff(
                            &pos_dir.join(filename),
                            &frame,
                            session.width,
                            session.height,
                        )?;
                        done += 1;
                        emit_progress(
                            on_progress,
                            ProgressPhase::Advance,
                            done,
                            total,
                            "Writing TIFFs",
                        );
                    }
                }
            }
        }

        emit_progress(
            on_progress,
            ProgressPhase::Finish,
            done,
            total,
            format!("Wrote {}", output.display()),
        );
        Ok(())
    }
}
