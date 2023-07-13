import argparse
from lkh import solve, LKHProblem
from tsplib95.models import StandardProblem

parser = argparse.ArgumentParser(
    prog='LKH-3 Interface',
    description='A CLI Program to initiate solving CVRP files with LKH-3',
    epilog='Made from Lucas Berger for scientific purposes')


parser.add_argument('tsplib_file')
parser.add_argument('--lkh-instance', default='../../bin/LKH')
parser.add_argument('--output-file')
parser.add_argument('-t', '--max-trials', default=1000)
parser.add_argument('-r', '--runs', default=10)

args = parser.parse_args()

    
    
problem = LKHProblem.load(args.tsplib_file)

extra = {}
tours = solve(args.lkh_instance, problem=problem, max_trials=args.max_trials, **extra)




if (args.output_file is not None):
    tour = StandardProblem()

    tour.tours = tours
    tour.type = "TOUR"
    tour.name = problem.name + " solution"

    tour.save(args.output_file)