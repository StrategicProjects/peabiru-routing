# Authors and Citation

## Authors

- **André Leite**. Author, maintainer.
  [](https://orcid.org/0000-0002-4718-9766)

- **Marcos Wasiliew**. Author. [](https://orcid.org/0009-0004-4694-3159)

- **Hugo Vasconcelos**. Author.
  [](https://orcid.org/0000-0001-6249-0920)

- **Carlos Amorim**. Author. [](https://orcid.org/0000-0001-6315-8305)

- **Diogo Bezerra**. Author. [](https://orcid.org/0000-0002-1216-8674)

- **Júlia Nascimento Barreto**. Author.
  [](https://orcid.org/0009-0004-2851-7770)

- **Conveyal LLC**. Copyright holder.\
  'R5' routing engine, from which the routing logic is ported

- **The Apache Software Foundation**. Copyright holder.\
  'Apache Commons Math', parts of which are translated in the 'Rust'
  core

- **Ipea - Institute for Applied Economic Research**. Copyright holder.\
  'r5r' package: 'Java' code translated in the 'Rust' core (fare
  calculator, parts of the trip planner), the function names, arguments
  and result layouts, and the sample data

- **r5r contributors**. Copyright holder.\
  Copyright holders named in the license of the 'r5r' package

- **Makoto Matsumoto**. Copyright holder.\
  Mersenne Twister generator, translated in the 'Rust' core

- **Takuji Nishimura**. Copyright holder.\
  Mersenne Twister generator, translated in the 'Rust' core

- **CERN - European Organization for Nuclear Research**. Copyright
  holder.\
  Table of prime numbers from the 'Colt' library, in the 'Rust' core

- **isoband authors**. Copyright holder.\
  'isoband' package, whose contouring algorithm is translated in the
  'Rust' core

- **CIESIN - Center for International Earth Science Information Network,
  Columbia University**. Copyright holder.\
  Population grid (GPWv3) by which the 'Rust' core checks GTFS stop
  locations

- **OpenStreetMap contributors**. Copyright holder.\
  Map data of the sample network, inst/extdata/spo/spo_osm.pbf

- **Authors of the dependency Rust crates**. Copyright holder.\
  Rust crates vendored in src/rust/vendor.tar.xz; see inst/AUTHORS

## Citation

Source:
[`DESCRIPTION`](https://github.com/StrategicProjects/peabiru-routing/blob/HEAD/DESCRIPTION)

Leite A, Wasiliew M, Vasconcelos H, Amorim C, Bezerra D, Nascimento
Barreto J (2026). *peabiru: Multimodal Routing on 'OpenStreetMap' and
'GTFS', a Port of 'R5'*. R package version 0.1.5,
<https://strategicprojects.github.io/peabiru-routing/r/>.

@Manual{,\
  title = {peabiru: Multimodal Routing on 'OpenStreetMap' and 'GTFS', a Port of 'R5'},\
  author = {André Leite and Marcos Wasiliew and Hugo Vasconcelos and Carlos Amorim and Diogo Bezerra and Júlia {Nascimento Barreto}},\
  year = {2026},\
  note = {R package version 0.1.5},\
  url = {https://strategicprojects.github.io/peabiru-routing/r/},\
}

## Additional details

The R package peabiru is written by the authors listed in its DESCRIPTION.\
Its compiled code is built with the following Rust crates, whose\
sources are included in the package once they are vendored for CRAN.\
The crates of this repository (peabiru-core) are by the package authors.\
Licence texts: inst/LICENSE-RUST-CRATES (the crates linked into the\
package's shared library) and the crates' own licence files.\
\
Generated by tools/third_party_notices.py from r/src/rust/Cargo.lock; do not edit.\
\
adler2 2.0.1\
  Licence:    0BSD OR MIT OR Apache-2.0\
  Authors:    Jonas Schievink \<jonasschievink@gmail.com\>\
              oyvindln \<oyvindln@users.noreply.github.com\>\
  Repository: https://github.com/oyvindln/adler2\
\
extendr-api 0.9.0\
  Licence:    MIT\
  Authors:    The Extendr contributors (CONTRIBUTORS.md: Andy Thomason, Thomas Down, Mossa Merhi Reimert, Claus O. Wilke, Hiroaki Yutani, Ilia Kosenkov, Daniel Falbel, Genomics PLC)\
  Repository: https://github.com/extendr/extendr\
\
extendr-ffi 0.9.0\
  Licence:    MIT\
  Authors:    The Extendr contributors (CONTRIBUTORS.md: Andy Thomason, Thomas Down, Mossa Merhi Reimert, Claus O. Wilke, Hiroaki Yutani, Ilia Kosenkov, Daniel Falbel, Genomics PLC)\
  Repository: https://github.com/extendr/extendr\
\
extendr-macros 0.9.0\
  Licence:    MIT\
  Authors:    The Extendr contributors (CONTRIBUTORS.md: Andy Thomason, Thomas Down, Mossa Merhi Reimert, Claus O. Wilke, Hiroaki Yutani, Ilia Kosenkov, Daniel Falbel, Genomics PLC)\
  Repository: https://github.com/extendr/extendr\
\
lazy_static 1.5.1\
  Licence:    MIT OR Apache-2.0\
  Authors:    Marvin Löbel \<loebel.marvin@gmail.com\>\
  Repository: https://github.com/rust-lang-nursery/lazy-static.rs\
\
miniz_oxide 0.9.1\
  Licence:    MIT OR Zlib OR Apache-2.0\
  Authors:    Frommi \<daniil.liferenko@gmail.com\>\
              oyvindln \<oyvindln@users.noreply.github.com\>\
              Rich Geldreich richgel99@gmail.com\
  Repository: https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide\
\
once_cell 1.21.4\
  Licence:    MIT OR Apache-2.0\
  Authors:    Aleksey Kladov \<aleksey.kladov@gmail.com\>\
  Repository: https://github.com/matklad/once_cell\
\
paste 1.0.15\
  Licence:    MIT OR Apache-2.0\
  Authors:    David Tolnay \<dtolnay@gmail.com\>\
  Repository: https://github.com/dtolnay/paste\
\
proc-macro2 1.0.107\
  Licence:    MIT OR Apache-2.0\
  Authors:    David Tolnay \<dtolnay@gmail.com\>\
              Alex Crichton \<alex@alexcrichton.com\>\
  Repository: https://github.com/dtolnay/proc-macro2\
\
quote 1.0.47\
  Licence:    MIT OR Apache-2.0\
  Authors:    David Tolnay \<dtolnay@gmail.com\>\
  Repository: https://github.com/dtolnay/quote\
\
readonly 0.2.14\
  Licence:    MIT OR Apache-2.0\
  Authors:    David Tolnay \<dtolnay@gmail.com\>\
  Repository: https://github.com/dtolnay/readonly\
\
syn 2.0.119\
  Licence:    MIT OR Apache-2.0\
  Authors:    David Tolnay \<dtolnay@gmail.com\>\
  Repository: https://github.com/dtolnay/syn\
\
syn 3.0.10\
  Licence:    MIT OR Apache-2.0\
  Authors:    David Tolnay \<dtolnay@gmail.com\>\
  Repository: https://github.com/dtolnay/syn\
\
unicode-ident 1.0.27\
  Licence:    (MIT OR Apache-2.0) AND Unicode-3.0\
  Authors:    David Tolnay \<dtolnay@gmail.com\>\
  Repository: https://github.com/dtolnay/unicode-ident
