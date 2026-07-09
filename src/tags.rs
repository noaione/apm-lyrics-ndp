use std::io::Seek;
use std::path::Path;

use lofty::aac::AacFile;
use lofty::ape::{ApeFile, ApeTag};
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType};
use lofty::flac::FlacFile;
use lofty::id3::v2::Id3v2Tag;
use lofty::iff::aiff::AiffFile;
use lofty::iff::wav::{RiffInfoList, WavFile};
use lofty::mpeg::MpegFile;
use lofty::musepack::MpcFile;
use lofty::ogg::{OpusFile, SpeexFile, VorbisComments, VorbisFile};
use lofty::wavpack::WavPackFile;
use mp4ameta::{Data, Fourcc};

type WantError = nd_pdk::lyrics::Error;

const VORBIS_CATALOG_ID_KEY: &str = "ITUNESCATALOGID";
const ID3_CATALOG_ID_DESCRIPTION: &str = "iTunes Catalog ID";
const CATALOG_KEYS: &[&str] = &["iTunesCatalogId", "ITUNESCATALOGID", "iTunes Catalog ID"];

fn minimal_options() -> ParseOptions {
    ParseOptions::new()
        .read_cover_art(false)
        .read_properties(false)
        .read_tags(true)
        .parsing_mode(lofty::config::ParsingMode::BestAttempt)
}

fn get_vorbis_catalog_id(tag: &VorbisComments) -> Option<String> {
    tag.get(VORBIS_CATALOG_ID_KEY).map(str::to_owned)
}

fn get_id3v2_catalog_id(tag: &Id3v2Tag) -> Option<String> {
    tag.get_user_text(ID3_CATALOG_ID_DESCRIPTION)
        .map(str::to_owned)
}

fn get_ape_catalog_id(tag: &ApeTag) -> Option<String> {
    CATALOG_KEYS
        .iter()
        .find_map(|key| tag.get(key)?.text_values()?.next().map(str::to_owned))
}

fn get_riff_catalog_id(tag: &RiffInfoList) -> Option<String> {
    CATALOG_KEYS
        .iter()
        .find_map(|key| tag.get(key).map(str::to_owned))
}

fn get_mp4_catalog_id(tag: &mp4ameta::Tag) -> Result<Option<String>, WantError> {
    let cnid = Fourcc(*b"cnID");

    if let Some(data) = tag.data_of(&cnid).next() {
        match data {
            Data::BeSigned(bytes) | Data::Reserved(bytes) => {
                let uint_data = be_uint(bytes);
                return Ok(uint_data.map(|uint| uint.to_string()));
            }
            Data::Utf8(enc_string) | Data::Utf16(enc_string) => {
                return Ok(Some(enc_string.to_string()));
            }
            other => {
                return Err(WantError::new(format!(
                    "mp4a_meta_read failed: unexpected data type: {:?}",
                    other
                )));
            }
        }
    }

    Ok(None)
}

fn be_uint(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || bytes.len() > 8 {
        return None;
    }

    let mut buf = [0u8; 8];
    buf[8 - bytes.len()..].copy_from_slice(bytes);
    Some(u64::from_be_bytes(buf))
}

pub(super) fn find_catalog_id(path: impl AsRef<Path>) -> Result<Option<String>, WantError> {
    let probe = lofty::probe::Probe::open(path)
        .map_err(|err| WantError::new(format!("lofty_read_file failed: {:?}", err)))?
        .guess_file_type()
        .map_err(|err| WantError::new(format!("lofty_probe failed: {:?}", err)))?;

    let ftype = probe
        .file_type()
        .ok_or_else(|| WantError::new("lofty_probe failed: no file type detected"))?;

    let mut reader = probe.into_inner();
    reader
        .rewind()
        .map_err(|err| WantError::new(format!("lofty_rewind failed: {:?}", err)))?;

    let opts = minimal_options();
    match ftype {
        FileType::Aac => {
            let tags = AacFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("aac_read_tags", err))?;

            Ok(tags.id3v2().and_then(get_id3v2_catalog_id))
        }
        FileType::Mpeg => {
            let tags = MpegFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("mpeg_read_tags", err))?;

            let value_id3v2 = tags.id3v2().and_then(get_id3v2_catalog_id);
            let value_ape = tags.ape().and_then(get_ape_catalog_id);

            Ok(value_id3v2.or(value_ape))
        }
        FileType::Flac => {
            let tags = FlacFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("flac_read_tags", err))?;

            let value_vorbis = tags.vorbis_comments().and_then(get_vorbis_catalog_id);
            let value_id3v2 = tags.id3v2().and_then(get_id3v2_catalog_id);

            Ok(value_vorbis.or(value_id3v2))
        }
        FileType::Ape => {
            let tags = ApeFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("ape_read_tags", err))?;

            let value_ape = tags.ape().and_then(get_ape_catalog_id);
            let value_id3v2 = tags.id3v2().and_then(get_id3v2_catalog_id);

            Ok(value_ape.or(value_id3v2))
        }
        FileType::Opus => {
            let tags = OpusFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("opus_read_tags", err))?;

            Ok(get_vorbis_catalog_id(tags.vorbis_comments()))
        }
        FileType::Vorbis => {
            let tags = VorbisFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("vorbis_read_tags", err))?;

            Ok(get_vorbis_catalog_id(tags.vorbis_comments()))
        }
        FileType::WavPack => {
            let tags = WavPackFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("wavpack_read_tags", err))?;

            Ok(tags.ape().and_then(get_ape_catalog_id))
        }
        FileType::Wav => {
            let tags = WavFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("wav_read_tags", err))?;

            let value_riff = tags.riff_info().and_then(get_riff_catalog_id);
            let value_id3v2 = tags.id3v2().and_then(get_id3v2_catalog_id);

            Ok(value_riff.or(value_id3v2))
        }
        FileType::Aiff => {
            let tags = AiffFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("aiff_read_tags", err))?;

            Ok(tags.id3v2().and_then(get_id3v2_catalog_id))
        }
        FileType::Speex => {
            let tags = SpeexFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("speex_read_tags", err))?;

            Ok(get_vorbis_catalog_id(tags.vorbis_comments()))
        }
        FileType::Mp4 => {
            let tags = mp4ameta::Tag::read_from(&mut reader)
                .map_err(|err| WantError::new(format!("speex_read_tags failed: {:?}", err)))?;
            get_mp4_catalog_id(&tags)
        }
        FileType::Mpc => {
            let tags = MpcFile::read_from(&mut reader, opts)
                .map_err(|err| lofty_string("mpc_read_tags", err))?;

            let value_ape = tags.ape().and_then(get_ape_catalog_id);
            let value_id3v2 = tags.id3v2().and_then(get_id3v2_catalog_id);

            Ok(value_ape.or(value_id3v2))
        }
        _ => Ok(None),
    }
}

fn lofty_string(when: impl Into<String>, error: lofty::error::LoftyError) -> nd_pdk::lyrics::Error {
    let formatted_err = format!("{} failed: {:?}", when.into(), error);
    WantError::new(formatted_err)
}
