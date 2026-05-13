import time

def main():
    n = 50
    a = 0
    b = 1
    next = b  
    count = 1

    while count <= n:
        print(next)
        count += 1
        a, b = b, next
        next = a + b
        time.sleep(.2)

if __name__ == "__main__":
    main()