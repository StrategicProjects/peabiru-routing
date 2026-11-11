// Records the iteration order of isoband's std::unordered_map under random
// insert / erase / clear sequences, for the tests of the emulated hash
// tables in src/isochrone/isoband.rs.
//
//   c++ -std=c++11 -O2 record.cpp -o record && ./record > <std library>.txt
//
// The key, its hasher and the way the map is used (operator[] to insert or
// overwrite, erase by key, clear between bands) are those of isoband 0.3.0
// (src/isoband.cpp). The operations come from splitmix64, which the Rust
// test reproduces, so the fixture holds only what the real container did:
//
//   seq <seed> <phases> <ops per phase> <rows> <cols>
//   cp (every 256 operations) <bucket count> <size> <FNV-1a 64 of the keys in iteration order>
//   end <bucket count> <size> <r,c,type of every key in iteration order>
//       (or `fnv <hash>` past 1500 keys)
//   grow <bucket count after each insertion that changed it, from empty>
//   policy <bucket counts of _Prime_rehash_policy, from 1, up to 60M keys>
//       (libstdc++ only)
#include <cstdint>
#include <cstdio>
#include <functional>
#include <unordered_map>

using namespace std;

enum point_type { grid, hintersect_lo, hintersect_hi, vintersect_lo, vintersect_hi };

struct grid_point {
  int r, c;
  point_type type;
  grid_point(double r_in = -1, double c_in = -1, point_type type_in = grid)
      : r(r_in), c(c_in), type(type_in) {}
  grid_point(const grid_point &p) : r(p.r), c(p.c), type(p.type) {}
};

struct grid_point_hasher {
  size_t operator()(const grid_point &p) const {
    return hash<long long>()((static_cast<long long>(p.r) << 30) ^
                             (static_cast<long long>(p.c) << 3) ^
                             static_cast<long long>(p.type));
  }
};

bool operator==(const grid_point &p1, const grid_point &p2) {
  return (p1.r == p2.r) && (p1.c == p2.c) && (p1.type == p2.type);
}

typedef unordered_map<grid_point, int, grid_point_hasher> gridmap;

static uint64_t state;
static uint64_t next_u64() {
  uint64_t z = (state += 0x9e3779b97f4a7c15ULL);
  z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
  z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
  return z ^ (z >> 31);
}

static uint64_t fnv(const gridmap &m) {
  uint64_t h = 0xcbf29ce484222325ULL;
  for (gridmap::const_iterator it = m.begin(); it != m.end(); ++it) {
    int32_t v[3] = {it->first.r, it->first.c, (int32_t)it->first.type};
    const unsigned char *b = (const unsigned char *)v;
    for (int i = 0; i < 12; i++) {
      h ^= b[i];
      h *= 0x100000001b3ULL;
    }
  }
  return h;
}

static void sequence(uint64_t seed, int phases, int ops, int rows, int cols) {
  printf("seq %llu %d %d %d %d\n", (unsigned long long)seed, phases, ops, rows, cols);
  state = seed;
  gridmap m;
  for (int ph = 0; ph < phases; ph++) {
    m.clear();
    for (int i = 0; i < ops; i++) {
      uint64_t op = next_u64() % 100;
      int r = (int)(next_u64() % (uint64_t)rows);
      int c = (int)(next_u64() % (uint64_t)cols);
      point_type t = (point_type)(next_u64() % 5);
      grid_point p(r, c, t);
      if (op < 65) {
        m[p] = i;
      } else {
        m.erase(p);
      }
      if (i % 256 == 255) {
        printf("cp %zu %zu %llu\n", m.bucket_count(), m.size(), (unsigned long long)fnv(m));
      }
    }
  }
  printf("end %zu %zu", m.bucket_count(), m.size());
  if (m.size() <= 1500) {
    for (gridmap::const_iterator it = m.begin(); it != m.end(); ++it) {
      printf(" %d,%d,%d", it->first.r, it->first.c, (int)it->first.type);
    }
  } else {
    printf(" fnv %llu", (unsigned long long)fnv(m));
  }
  printf("\n");
}

int main() {
  // few keys (many erase hits), mid-sized and large ranges (growth)
  sequence(1, 3, 2000, 8, 8);
  sequence(2, 4, 1500, 20, 20);
  sequence(3, 2, 4000, 60, 40);
  sequence(4, 3, 3000, 1000, 1000);
  sequence(5, 1, 20000, 300, 200);
  sequence(6, 6, 700, 25, 30);
  sequence(7, 2, 9000, 70, 70);
  sequence(8, 1, 30000, 5000, 3);
  // bucket counts from empty, keys inserted in isoband's raster order
  gridmap m;
  size_t last = m.bucket_count();
  printf("grow %zu", last);
  for (int i = 0; i < 400000; i++) {
    m[grid_point(i / 700, i % 700, grid)] = i;
    if (m.bucket_count() != last) {
      last = m.bucket_count();
      printf(" %zu", last);
    }
  }
  printf("\n");
#ifdef __GLIBCXX__
  // bucket counts libstdc++'s rehash policy (the one the container above
  // calls on every insertion) gives, one key at a time, up to 60,000,000
  // keys: past what a grid of isoband points can hold, without building
  // the table
  std::__detail::_Prime_rehash_policy policy;
  size_t bkt = 1;
  printf("policy %zu", bkt);
  for (size_t n = 0; n < 60000000; n++) {
    std::pair<bool, size_t> r = policy._M_need_rehash(bkt, n, 1);
    if (r.first) {
      bkt = r.second;
      printf(" %zu", bkt);
    }
  }
  printf("\n");
#endif
  return 0;
}
