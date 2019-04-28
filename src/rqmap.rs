use rust_htslib::bam;
use bio::data_structures::annot_map::*;
use bio_types::annot::contig::Contig;
use bio_types::annot::pos::Pos;
use bio_types::annot::loc::Loc;
use bio_types::strand::ReqStrand;
use rust_htslib::bam::HeaderView;
use crate::locus::from_rec::*;
use crate::scaffold_dict::ScaffoldDict;
use crate::locus::positions::PositionScan;
use rust_htslib::bam::Read;
use std::cmp::{min, max};

trait IntoAnnotMap{
    fn add_to(self, am: &mut AnnotMap<String, Contig<String,ReqStrand>>, sd: &ScaffoldDict, pf: Option<fn(Contig<String,ReqStrand>) -> Contig<String,ReqStrand>>) -> Result<(), &'static str>;
}


impl<T: Iterator<Item=bam::Record>> IntoAnnotMap for T {
    fn add_to(self, am: &mut AnnotMap<String, Contig<String,ReqStrand>>, sd: &ScaffoldDict, pf: Option<fn(Contig<String,ReqStrand>) -> Contig<String,ReqStrand>>) -> Result<(), &'static str> {

        match pf {
            None => {self.map(|a| Contig::from_read(&a, false, sd))
                         .map(|a| a.unwrap())
                         .for_each(|a| am.insert_loc(a));},
            Some(f) => {self.map(|a| Contig::from_read(&a, false, sd))
                            .map(|a| a.unwrap())
                            .map(|a| f(a)).for_each(|a| am.insert_loc(a));},
        }

        Ok(())
    }
}


trait AppendRecord {
    fn append(&mut self, r: &bam::Record, sd: &ScaffoldDict, pf: &Option<fn(Contig<String,ReqStrand>)-> Contig<String,ReqStrand>>) -> Result<(), &'static str>;
}

impl AppendRecord for AnnotMap<String, Contig<String,ReqStrand>> {
    fn append(&mut self, r: &bam::Record, sd: &ScaffoldDict, pf: &Option<fn(Contig<String,ReqStrand>)-> Contig<String,ReqStrand>>) -> Result<(), &'static str> {

        match pf {
            None => self.insert_loc(Contig::from_read(r, false, sd).unwrap()),
            Some(f) => self.insert_loc(f(Contig::from_read(r, false, sd).unwrap())),
        }

        Ok(())
    }
}


#[derive(Debug)]
pub enum LibraryType {
    R1Sense,
    R2Sense,
    Unstranded,
}

/// Struct holds a library of NGS reads as an AnnotMap, as well as the original reader struct
/// and strandedness information.
#[derive(Debug)]
pub struct RQMap<T>
    where T: bam::Read {
    reader: T,
    construction: LibraryType,
    map: AnnotMap<String, Contig<String,ReqStrand>>,
}

impl<T: Read> RQMap<T> {
    /// Retrieve the coverage at a pointlike locus.
    pub fn coverage_at<D: Into<ReqStrand>>(&self, p: &Pos<String, D>) -> usize {
        self.map.find(p).count()
    }
    /// Retrieve the coverage across a contiguous locus.
    pub fn coverage_across(&self, c: &Contig<String, ReqStrand>) -> Vec<usize> {
        c.positions()
         .map(|a| a.unwrap())
         .map(|a| self.coverage_at(&a))
         .collect()
    }
}


impl RQMap<bam::Reader> {
    /// TODO add filter for raw reads
    // pub fn from_reader(mut b: bam::Reader, lt: LibraryType, pf: Option<fn(Contig<String,ReqStrand>) -> Contig<String,ReqStrand>>) -> Self {
    //     let mut map: AnnotMap<String,Contig<String,ReqStrand>> = AnnotMap::new();
    //     let hd: HeaderView = b.header().clone();
    //     let sd: ScaffoldDict = ScaffoldDict::from_header_view(&hd);
    //     let _r = b.records()
    //                 .map(|a| a.unwrap())
    //                 .add_to(&mut map, &sd, pf);
    //     RQMap {
    //         reader: b,
    //         construction: lt,
    //         map: map,
    //     }
    // }
    pub fn from_reader(mut b: bam::Reader, lt: LibraryType, pf: Option<fn(Contig<String,ReqStrand>) -> Contig<String,ReqStrand>>) -> Self {
        let mut map: AnnotMap<String,Contig<String,ReqStrand>> = AnnotMap::new();
        let hd: HeaderView = b.header().clone();
        let sd: ScaffoldDict = ScaffoldDict::from_header_view(&hd);
        let mut r: bam::Record = bam::Record::new();

        while let Ok(_r) = b.read(&mut r) {
            map.append(&r, &sd, &pf);
        }

        RQMap {
            reader: b,
            construction: lt,
            map: map,
        }
    }


}

impl RQMap<bam::IndexedReader> {
    /// Create an RQMap from an indexed bam reader.
    pub fn from_indexed(mut b: bam::IndexedReader, c: Vec<Contig<String,ReqStrand>>, lt: LibraryType, pf: Option<fn(Contig<String,ReqStrand>) -> Contig<String,ReqStrand>>) -> Self {
        let mut map: AnnotMap<String,Contig<String,ReqStrand>> = AnnotMap::new();
        let hd: HeaderView = b.header().clone();
        let sd: ScaffoldDict = ScaffoldDict::from_header_view(&hd);

        for x in c.into_iter() {
            let chr = match sd.str_to_id(&x.refid()) {
                Some(i) => i as u32,
                None => continue,
            };
            let c1 = x.first_pos().start() as u32;
            let c2 = x.last_pos().start() as u32;
            b.fetch(chr, min(c1,c2), max(c1,c2));
            b.records()
             .map(|a| a.unwrap())
             .add_to(&mut map, &sd, pf);
        }

        RQMap {
            reader: b,
            construction: lt,
            map: map,
        }
    }
}




#[cfg(test)]
mod tests {
    use bio::data_structures::annot_map::AnnotMap;
    use rust_htslib::bam;
    use std::path::Path;
    use rust_htslib::bam::Read;
    use bio_types::annot::contig::Contig;
    use bio_types::annot::loc::Loc;
    use crate::rqmap::*;
    use crate::locus::shift::*;
    use rust_htslib::bam::HeaderView;
    use bio_types::strand::ReqStrand;

    fn tn5shift(c: Contig<String,ReqStrand>) -> Contig<String,ReqStrand> {
    let new = match c.strand() {
        ReqStrand::Forward => c.shift(4),
        ReqStrand::Reverse => c.shift(5),
    };
    new.first_pos().contig()
    }

    #[test]
    fn reads_into_annotmap() {
        let mut map: AnnotMap<String,Contig<String,ReqStrand>> = AnnotMap::new();
        let bampath = Path::new("test/hs.pe.test.bam");
        let mut bam = bam::Reader::from_path(bampath).unwrap();
        let hd: HeaderView = bam.header().clone();
        let sd: ScaffoldDict = ScaffoldDict::from_header_view(&hd);
        let res = bam
            .records()
            .take(5)
            .map(|a| a.unwrap())
            .add_to(&mut map, &sd, None);

        // TODO work on this test
        assert_eq!(res, Ok(()))
    }

    #[test]
    fn reads_into_ngslib() {
        let bampath = Path::new("test/hs.pe.test.bam");
        let bam = bam::Reader::from_path(bampath).unwrap();

        // TODO Work on this test
        let _r = RQMap::from_reader(bam, LibraryType::Unstranded, None);

    }

    #[test]
    fn reads_into_ngslib_preproc() {
        let bampath = Path::new("test/hs.pe.test.bam");
        let bam = bam::Reader::from_path(bampath).unwrap();


        // TODO work on this test
        let _r = RQMap::from_reader(bam, LibraryType::Unstranded, Some(tn5shift));

    }

    #[test]
    fn indexed_reads_into_ngslib() {
        let bampath = Path::new("test/hs.pe.test.bam");
        let bam = bam::IndexedReader::from_path(bampath).unwrap();


        let c1: Contig<String,ReqStrand> = Contig::new("chr1".to_string(),
                                                     1000000,
                                                     1000000,
                                                     ReqStrand::Forward);

        let _r = RQMap::from_indexed(bam,
                                         vec!(c1),
                                         LibraryType::Unstranded,
                                         None);
        }

    #[test]
    fn coverage_across_region() {
        let bampath = Path::new("test/hs.pe.test.bam");
        let bam = bam::IndexedReader::from_path(bampath).unwrap();

        let c0: Contig<String,ReqStrand> = Contig::new("chr1".to_string(),
                                                     564475,
                                                     60,
                                                     ReqStrand::Forward);

        let r = RQMap::from_indexed(bam,
                                        vec!(c0.clone()),
                                        LibraryType::Unstranded,
                                        Some(tn5shift));
        println!("COV ACROSS: {:?}", r);

        println!("COV ACROSS: {:?}", r.coverage_across(&c0));

        }

}
