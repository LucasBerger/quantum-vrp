import numpy as np
from qrisp import *

city_amount = 4

distance_matrix = (
    np.array(
        [
            [0, 0.25, 0.125, 0.5],
            [0.25, 0, 0.625, 0.375],
            [0.125, 0.625, 0, 0.75],
            [0.5, 0.375, 0.75, 0],
        ]
    )
    / 4
)


# Create a function that generates a state of superposition of all permutations
def swap_to_front(qa, index):
    with invert():
        # The keyword ctrl_method = "gray_pt" allows the controlled swaps to be synthesized
        # using Margolus gates. These gates perform the same operation as a regular Toffoli
        # but add a different phase for each input. This phase will not matter though,
        # since it will be reverted once the ancilla values of the oracle are uncomputed.
        demux(qa[0], index, qa, permit_mismatching_size=True, ctrl_method="gray_pt")


def eval_perm(perm_specifiers):
    N = len(perm_specifiers)

    # To filter out the cyclic permutations, we impose that the first city is always city 0
    # We will have to consider this assumption later when calculating the route distance
    # by manually adding the trip distance of the first trip (from city 0) and the
    # last trip (to city 0)
    qa = QuantumArray(QuantumFloat(int(np.ceil(np.log2(city_amount)))), city_amount - 1)

    qa[:] = np.arange(1, city_amount)

    for i in range(N):
        swap_to_front(qa[i:], perm_specifiers[i])

    return qa


# Create function that returns QuantumFloats specifying the permutations (these will be in uniform superposition)
def create_perm_specifiers(city_amount, init_seq=None):
    perm_specifiers = []

    for i in range(city_amount - 1):
        qf_size = int(np.ceil(np.log2(city_amount - i)))

        if i == 0:
            continue

        temp_qf = QuantumFloat(qf_size)

        if not init_seq is None:
            temp_qf[:] = init_seq[i - 1]

        perm_specifiers.append(temp_qf)

    return perm_specifiers


# Create function that evaluates if a certain permutation is below a certain distance


# First implement distance function
@as_hamiltonian
def trip_distance(i, j, iter=1):
    return distance_matrix[i, j] * 2 * np.pi * iter


@as_hamiltonian
def distance_to_0(j, iter=1):
    return distance_matrix[0, j] * 2 * np.pi * iter


def phase_apply_summed_distance(itinerary, iter=1):
    # Add the distance of the first trip
    distance_to_0(itinerary[0], iter=iter)

    # Add the distance of the last trip
    distance_to_0(itinerary[-1], iter=iter)

    # Add the remaining trips
    for i in range(city_amount - 2):
        trip_distance(itinerary[i], itinerary[i + 1], iter=iter)


@lifted
def qpe_calc_perm_travel_distance(itinerary, precision):
    if precision is None:
        raise Exception("Tried to evaluate oracle without specifying a precision")

    return QPE(
        itinerary, phase_apply_summed_distance, precision=precision, iter_spec=True
    )


def qdict_calc_perm_travel_distance(itinerary, precision):
    # A QuantumFloat with n qubits and exponent -n
    # can represent values between 0 and 1
    res = QuantumFloat(precision, -precision)

    # Fill QuantumDictionary
    qd = QuantumDictionary(return_type=res)
    for i in range(city_amount):
        for j in range(city_amount):
            qd[(i, j)] = distance_matrix[i, j]

    # This dictionary contains the distances of each city to city 0
    qd_to_zero = QuantumDictionary(return_type=res)

    for i in range(city_amount):
        qd_to_zero[i] = distance_matrix[0, i]

    # The distance of the first trip is acquired by loading from qd_to_zero
    res = qd_to_zero[itinerary[0]]

    # Add the distance of the final trip
    final_trip_distance = qd_to_zero[itinerary[-1]]
    res += final_trip_distance
    final_trip_distance.uncompute(recompute=True)

    # Evaluate result
    for i in range(city_amount - 2):
        trip_distance = qd[itinerary[i], itinerary[(i + 1) % city_amount]]
        res += trip_distance
        trip_distance.uncompute(recompute=True)

    return res


@auto_uncompute
def eval_distance_threshold(perm_specifiers, precision, threshold, method="qpe"):
    itinerary = eval_perm(perm_specifiers)

    if method == "qdict":
        distance = qdict_calc_perm_travel_distance(itinerary, precision)
    elif method == "qpe":
        distance = qpe_calc_perm_travel_distance(itinerary, precision)
    else:
        raise Exception(f"Don't know method {method}")

    is_below_treshold = distance <= threshold

    z(is_below_treshold)


# Create permutation specifiers
perm_specifiers = create_perm_specifiers(city_amount)

for qv in perm_specifiers:
    h(qv)


eval_distance_threshold(perm_specifiers, 5, 0.4, "qdict")

print(multi_measurement(perm_specifiers))


from math import factorial

from qrisp.grover import grovers_alg

winner_state_amount = 2 ** sum([qv.size for qv in perm_specifiers]) / factorial(
    city_amount - 2
)  # average number of state per permutation * (4 cyclic shifts)*(2 directions)


# Evaluate Grovers algorithm
grovers_alg(
    perm_specifiers,  # Permutation specifiers
    eval_distance_threshold,  # Oracle function
    kwargs={
        "threshold": 0.4,
        "precision": 5,
        "method": "qdict",
    },  # Specify the keyword arguments for the Oracle
    winner_state_amount=winner_state_amount,
)  # Specify the estimated amount of winners

# Retrieve measurement
res = multi_measurement(perm_specifiers)

print(res)
