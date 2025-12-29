def fibonacci(n):
    """Generate Fibonacci sequence up to n terms."""
    fib_sequence = []
    a, b = 0, 1
    for _ in range(n):
        fib_sequence.append(a)
        a, b = b, a + b
    return fib_sequence

# Example usage
terms = 10
print(f"Fibonacci sequence of {terms} terms: {fibonacci(terms)}")
