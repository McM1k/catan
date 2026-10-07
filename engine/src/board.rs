use crate::Resource;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Terrain {
    Forest,
    Hills,
    Pasture,
    Fields,
    Mountains,
    Desert,
}

impl Terrain {
    pub fn resource(self) -> Option<Resource> {
        match self {
            Terrain::Forest => Some(Resource::Wood),
            Terrain::Hills => Some(Resource::Brick),
            Terrain::Pasture => Some(Resource::Sheep),
            Terrain::Fields => Some(Resource::Wheat),
            Terrain::Mountains => Some(Resource::Ore),
            Terrain::Desert => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tile {
    pub q: i32,
    pub r: i32,
    /// Center in board units (hex size 1.0, pointy-top).
    pub center: (f64, f64),
    pub terrain: Terrain,
    pub number: Option<u8>,
    pub vertices: [usize; 6],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vertex {
    pub pos: (f64, f64),
    pub tiles: Vec<usize>,
    pub neighbors: Vec<usize>,
    pub edges: Vec<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    pub coastal: bool,
}

/// `None` kind is a generic 3:1 port, `Some(r)` is a 2:1 port for `r`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Port {
    pub edge: usize,
    pub kind: Option<Resource>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Board {
    pub tiles: Vec<Tile>,
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
    pub ports: Vec<Port>,
}

const HEX_RADIUS: i32 = 2;

impl Board {
    /// Generates a random standard-sized board (19 land hexes, 9 ports).
    pub fn generate(rng: &mut impl Rng) -> Board {
        let mut terrains = Vec::new();
        for (t, n) in [
            (Terrain::Forest, 4),
            (Terrain::Pasture, 4),
            (Terrain::Fields, 4),
            (Terrain::Hills, 3),
            (Terrain::Mountains, 3),
            (Terrain::Desert, 1),
        ] {
            for _ in 0..n {
                terrains.push(t);
            }
        }
        terrains.shuffle(rng);

        let mut coords = Vec::new();
        for r in -HEX_RADIUS..=HEX_RADIUS {
            for q in -HEX_RADIUS..=HEX_RADIUS {
                if (q + r).abs() <= HEX_RADIUS {
                    coords.push((q, r));
                }
            }
        }

        let sqrt3 = 3f64.sqrt();
        let mut tiles: Vec<Tile> = coords
            .iter()
            .zip(terrains)
            .map(|(&(q, r), terrain)| Tile {
                q,
                r,
                center: (sqrt3 * (q as f64 + r as f64 / 2.0), 1.5 * r as f64),
                terrain,
                number: None,
                vertices: [0; 6],
            })
            .collect();

        // Vertices and edges, deduplicated by rounded position.
        let mut vkeys: HashMap<(i64, i64), usize> = HashMap::new();
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut ekeys: HashMap<(usize, usize), usize> = HashMap::new();
        let mut edges: Vec<Edge> = Vec::new();
        let mut edge_tile_count: Vec<u8> = Vec::new();

        for (ti, tile) in tiles.iter_mut().enumerate() {
            let mut ids = [0usize; 6];
            for (i, id) in ids.iter_mut().enumerate() {
                let ang = (60.0 * i as f64 - 30.0).to_radians();
                let x = tile.center.0 + ang.cos();
                let y = tile.center.1 + ang.sin();
                let key = ((x * 100.0).round() as i64, (y * 100.0).round() as i64);
                let vi = *vkeys.entry(key).or_insert_with(|| {
                    vertices.push(Vertex {
                        pos: (x, y),
                        tiles: Vec::new(),
                        neighbors: Vec::new(),
                        edges: Vec::new(),
                    });
                    vertices.len() - 1
                });
                if !vertices[vi].tiles.contains(&ti) {
                    vertices[vi].tiles.push(ti);
                }
                *id = vi;
            }
            tile.vertices = ids;
            for i in 0..6 {
                let (a, b) = (ids[i], ids[(i + 1) % 6]);
                let key = (a.min(b), a.max(b));
                let ei = *ekeys.entry(key).or_insert_with(|| {
                    edges.push(Edge {
                        a: key.0,
                        b: key.1,
                        coastal: false,
                    });
                    edge_tile_count.push(0);
                    edges.len() - 1
                });
                edge_tile_count[ei] += 1;
            }
        }

        for (ei, e) in edges.iter_mut().enumerate() {
            e.coastal = edge_tile_count[ei] == 1;
        }
        for (ei, e) in edges.iter().enumerate() {
            vertices[e.a].neighbors.push(e.b);
            vertices[e.b].neighbors.push(e.a);
            vertices[e.a].edges.push(ei);
            vertices[e.b].edges.push(ei);
        }

        assign_numbers(&mut tiles, rng);

        // Ports: spread 9 around the 30 coastal edges, ordered by angle.
        let mut coastal: Vec<usize> = (0..edges.len()).filter(|&i| edges[i].coastal).collect();
        coastal.sort_by(|&x, &y| {
            let ang = |i: usize| {
                let e = &edges[i];
                let (pa, pb) = (vertices[e.a].pos, vertices[e.b].pos);
                ((pa.1 + pb.1) / 2.0).atan2((pa.0 + pb.0) / 2.0)
            };
            ang(x).partial_cmp(&ang(y)).unwrap()
        });
        let mut kinds: Vec<Option<Resource>> = vec![None, None, None, None];
        kinds.extend(Resource::ALL.iter().map(|&r| Some(r)));
        kinds.shuffle(rng);
        let n = coastal.len();
        let ports = kinds
            .into_iter()
            .enumerate()
            .map(|(i, kind)| Port {
                edge: coastal[(i * n / 9) % n],
                kind,
            })
            .collect();

        Board {
            tiles,
            vertices,
            edges,
            ports,
        }
    }

    pub fn desert(&self) -> usize {
        self.tiles
            .iter()
            .position(|t| t.terrain == Terrain::Desert)
            .unwrap()
    }

    /// Port available at each vertex (None = no port; Some(None) = 3:1; Some(Some(r)) = 2:1).
    pub fn vertex_ports(&self) -> Vec<Option<Option<Resource>>> {
        let mut out = vec![None; self.vertices.len()];
        for p in &self.ports {
            let e = &self.edges[p.edge];
            out[e.a] = Some(p.kind);
            out[e.b] = Some(p.kind);
        }
        out
    }

    pub fn tile_neighbors(&self, ti: usize) -> Vec<usize> {
        let t = &self.tiles[ti];
        self.tiles
            .iter()
            .enumerate()
            .filter(|(i, o)| {
                *i != ti && {
                    let (dq, dr) = (o.q - t.q, o.r - t.r);
                    matches!((dq, dr), (1, 0) | (-1, 0) | (0, 1) | (0, -1) | (1, -1) | (-1, 1))
                }
            })
            .map(|(i, _)| i)
            .collect()
    }
}

fn assign_numbers(tiles: &mut [Tile], rng: &mut impl Rng) {
    let mut pool: Vec<u8> = vec![2, 3, 3, 4, 4, 5, 5, 6, 6, 8, 8, 9, 9, 10, 10, 11, 11, 12];
    let land: Vec<usize> = (0..tiles.len())
        .filter(|&i| tiles[i].terrain != Terrain::Desert)
        .collect();

    // Retry until no two "hot" numbers (6/8) touch.
    for _ in 0..2000 {
        pool.shuffle(rng);
        for (k, &ti) in land.iter().enumerate() {
            tiles[ti].number = Some(pool[k]);
        }
        let hot = |t: &Tile| matches!(t.number, Some(6) | Some(8));
        let ok = land.iter().all(|&i| {
            !hot(&tiles[i])
                || land.iter().all(|&j| {
                    i == j || !hot(&tiles[j]) || {
                        let (dq, dr) = (tiles[j].q - tiles[i].q, tiles[j].r - tiles[i].r);
                        !matches!((dq, dr), (1, 0) | (-1, 0) | (0, 1) | (0, -1) | (1, -1) | (-1, 1))
                    }
                })
        });
        if ok {
            return;
        }
    }
}
