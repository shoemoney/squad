//! Deterministic contiguous quote matching, with original UTF-8 byte offsets.
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;
fn normalized(text: &str) -> (String, Vec<(usize,usize)>) {
 let mut out=String::new(); let mut spans=Vec::new();
 for (start,cluster) in text.grapheme_indices(true) {
  let end=start+cluster.len();
  for c in cluster.nfkc() {
   let c=match c { '‘'|'’'|'‚'|'‛' => '\'', '“'|'”'|'„'|'‟' => '"', '–'|'—' => '-', c => c };
   if c.is_whitespace() {
    if out.is_empty() {continue;}
    if out.ends_with(' ') { if let Some(last)=spans.last_mut() { let span: &mut (usize,usize)=last; span.1=end; } continue; }
    out.push(' ');spans.push((start,end));
   } else {out.push(c); for _ in 0..c.len_utf8() {spans.push((start,end));}}
  }
 }
 if out.ends_with(' ') {out.pop();spans.pop();}
 (out,spans)
}
pub fn verbatim(quote: &str, source: &str) -> Option<std::ops::Range<usize>> {
 let (quote,_)=normalized(quote);
 if quote.chars().count()<12 || quote.contains("...") || quote.contains('…') {return None;}
 let (source,spans)=normalized(source);
 let start=source.find(&quote)?; let end=start+quote.len();
 Some(spans.get(start)?.0..spans.get(end-1)?.1)
}
