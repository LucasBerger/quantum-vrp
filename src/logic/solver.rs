use crate::logic::{solvers::VRPTourWriter, util};

use super::{
    super::error_code::ExitCode,
    clustering::{ClusterOutput, ClusteringTrait},
    solvers::{SolvingOutput, SolvingTrait},
};

use bimap::BiMap;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::Path,
    process::exit,
    time::SystemTime,
};
use tspf::{Point, Tsp, TspBuilder, TspKind, TspSerializer};

fn reindex_vrp(vrp: &Tsp) -> (Tsp, BiMap<usize, usize>) {
    let mut pts = vrp.node_coords().values().collect::<Vec<&Point>>();

    pts.sort_by_key(|point| point.id());

    let map: BiMap<usize, usize> = pts
        .iter()
        .enumerate()
        .map(|(new_id, pt)| (pt.id(), new_id + 1))
        .collect();

    (
        Tsp::from(
            vrp.name().to_string(),
            vrp.kind(),
            vrp.comment().to_string(),
            vrp.dim(),
            vrp.capacity(),
            vrp.weight_kind(),
            vrp.weight_format(),
            vrp.edge_format().clone(),
            vrp.coord_kind(),
            vrp.disp_kind(),
            vrp.node_coords()
                .iter()
                .map(|(id, p)| {
                    let new_id = map.get_by_left(id).unwrap();
                    (*new_id, Point::new(*new_id, p.pos().to_vec()))
                })
                .collect(),
            vrp.depots()
                .iter()
                .map(|id| {
                    let new_id = map.get_by_left(id).unwrap();
                    *new_id
                })
                .collect(),
            vrp.demands()
                .iter()
                .map(|(id, demand)| {
                    let new_id = map.get_by_left(id).unwrap();
                    (*new_id, *demand)
                })
                .collect(),
            vrp.fixed_edges().to_vec(),
            vrp.disp_coords().to_vec(),
            vrp.edge_weights().to_vec(),
            vec![],
        ),
        map,
    )
}

pub struct VrpSolver {
    pub cluster_strat: Box<dyn ClusteringTrait>,
    pub solving_strat: Box<dyn SolvingTrait>,
    pub build_dir: Option<String>,
}
impl VrpSolver {
    pub fn partial_cluster(
        &self,
        path: &str,
        problem: &Tsp,
    ) -> Vec<(File, String, BiMap<usize, usize>)> {
        let file_name = Path::new(path).file_stem().unwrap().to_str().unwrap();

        let start_time = SystemTime::now();
        let clusters = self.cluster_strat.cluster(problem);
        println!("{:?}", clusters);
        let vrps_raw = self.cluster_tsps(problem, clusters);

        let after_cluster_time = SystemTime::now()
            .duration_since(start_time)
            .unwrap()
            .as_secs_f32();
        println!("clustered after: {after_cluster_time}");

        let vrps: Vec<(usize, Tsp, BiMap<usize, usize>)> = vrps_raw
            .iter()
            .map(|(i, tsp)| {
                let mapped = reindex_vrp(tsp);
                (*i, mapped.0, mapped.1)
            })
            .collect();

        let build_dir = self.build_dir();

        match fs::create_dir_all(&build_dir) {
            Ok(_) => {}
            Err(e) => {
                println!("Something went wrong creating dirs: \n{e}");
                exit(1)
            }
        };

        // serialize reindex map
        let map_file_path = format!("{}/{}.map", build_dir, file_name);
        let mut map_file = match std::fs::File::create(&map_file_path) {
            Ok(file) => file,
            Err(e) => {
                println!("Problem opening map file {e}");
                exit(1)
            }
        };
        for (_, _, map) in &vrps {
            let map = map
                .iter()
                .filter(|(i, _)| **i != 1usize)
                .map(|(k, v)| format!("{k} {v}"))
                .collect::<Vec<String>>()
                .join("\n");
            write!(map_file, "{map}\n-1\n").unwrap();
        }

        vrps.iter()
            .map(|(i, vrp, map)| {
                let (file, path) = match TspSerializer::serialize_file(
                    vrp,
                    format!("{}/{}_{}.vrp", build_dir, &file_name, i),
                ) {
                    Err(err) => {
                        println!("{}", err);
                        exit(1)
                    }
                    Ok(file) => file,
                };
                (file, path, map.clone())
            })
            .collect()
    }
    fn cluster_tsps(&self, problem: &Tsp, clusters: ClusterOutput) -> Vec<(usize, Tsp)> {
        let mut tsps: Vec<(usize, Tsp)> = Vec::new();
        for (i, cluster) in clusters.iter().enumerate() {
            let node_coords = problem
                .node_coords()
                .iter()
                .filter(|(u, _point)| cluster.contains(*u) || problem.depots().contains(*u))
                .map(|(u, p)| (*u, p.clone()))
                .collect();

            let node_coords_filter: HashMap<usize, &Point> = problem
                .node_coords()
                .iter()
                .filter(|(u, _point)| cluster.contains(*u) || problem.depots().contains(*u))
                .map(|(u, p)| (*u, p))
                .collect();

            let tsp = Tsp::from(
                format!("{}_{}", problem.name(), i.to_string()),
                tspf::TspKind::Cvrp,
                format!("{} - Cluster Nr.{}", problem.comment(), i.to_string()),
                problem.depots().len() + cluster.len(),
                problem.capacity(),
                problem.weight_kind(),
                problem.weight_format(),
                problem.edge_format().clone(),
                problem.coord_kind(),
                problem.disp_kind(),
                node_coords,
                problem.depots().iter().copied().collect(),
                problem
                    .demands()
                    .iter()
                    .filter(|(u, _p)| node_coords_filter.get(u).is_some())
                    .map(|(u, p)| (*u, *p))
                    .collect(),
                problem
                    .fixed_edges()
                    .iter()
                    .filter(|edge| {
                        node_coords_filter.get(&edge.0).is_some()
                            && node_coords_filter.get(&edge.1).is_some()
                    })
                    .copied()
                    .collect(),
                problem
                    .disp_coords()
                    .iter()
                    .filter_map(|p| node_coords_filter.get(&p.id()).map(|_| p.clone()))
                    .collect(),
                problem
                    .edge_weights()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, vec)| {
                        node_coords_filter.get(&i).map(|_i| {
                            vec.iter()
                                .enumerate()
                                .filter_map(|(j, poi)| node_coords_filter.get(&j).map(|_| *poi))
                                .collect::<Vec<f64>>()
                        })
                    })
                    .collect(),
                vec![],
            );
            tsps.push((i, tsp));
        }
        tsps
    }
    fn build_dir(&self) -> String {
        if let Some(dir) = &self.build_dir {
            dir.clone()
        } else {
            String::from("./.vrp")
        }
    }
}

impl SolvingTrait for VrpSolver {
    fn solve(&self, path: &str, transform_only: Option<bool>) -> SolvingOutput {
        let problem = match TspBuilder::parse_path(path) {
            Ok(instance) => instance,
            Err(e) => {
                println!("Problems reading the VRP-Instance: {}", e);
                exit(ExitCode::ReadProblems as i32);
            }
        };
        if problem.kind() != TspKind::Cvrp {
            println!(
                "Invalid TSPLIB instance type {}. (supported is CVRP)",
                problem.kind().to_string().to_uppercase()
            );
            exit(ExitCode::WrongTspType as i32);
        }

        println!("name: {}", problem.name());
        println!("type: {}", problem.kind());

        let start_time = SystemTime::now();
        println!("start");
        let vrps = self.partial_cluster(path, &problem);

        let solver_start = SystemTime::now()
            .duration_since(start_time)
            .unwrap()
            .as_secs_f32();
        println!("start solving clustered vrps: {solver_start}");

        let all_paths = vrps
            .iter()
            .map(|(_file, path, map)| {
                let before_solve_time = SystemTime::now();
                println!("solve {path} start");

                let paths = self.solving_strat.solve(&path[..], transform_only);

                let after_solve_time = SystemTime::now()
                    .duration_since(before_solve_time)
                    .unwrap()
                    .as_secs_f32();
                println!("solve {path} end: {after_solve_time}");

                let paths = paths
                    .output()
                    .iter()
                    .map(|path| {
                        path.iter()
                            .map(|id| *map.get_by_right(id).unwrap())
                            .collect()
                    })
                    .collect();

                SolvingOutput::new(paths)
            })
            .reduce(|paths, new_paths| {
                let combined_paths: [Vec<Vec<usize>>; 2] = [paths.into(), new_paths.into()];
                SolvingOutput::new(combined_paths.concat())
            })
            .unwrap_or_else(|| {
                println!("Something went wrong reducing paths");
                SolvingOutput::new(vec![])
            });

        let finished = SystemTime::now()
            .duration_since(start_time)
            .unwrap()
            .as_secs_f32();
        println!("finished: {finished}");

        let sol_length = util::tsp::calculate_solution_score(&problem, &all_paths.output());

        println!("length: {sol_length}");

        let file_dir = Path::new(path).parent().unwrap().to_str().unwrap();
        let file_name = Path::new(path).file_stem().unwrap().to_str().unwrap();

        let mut file = match std::fs::File::create(format!("{}/{}.sol", file_dir, file_name)) {
            Ok(file) => file,
            Err(e) => {
                println!("Problem opening solution file {e}");
                exit(1)
            }
        };

        println!("writing tours to file {file_dir}/{file_name}.sol");
        (&problem, &all_paths).write_tours(&mut file).unwrap();

        all_paths
    }
}
