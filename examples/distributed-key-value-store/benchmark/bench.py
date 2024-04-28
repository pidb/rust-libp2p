import grpc
import distributedkv_pb2
import distributedkv_pb2_grpc
import time
import threading
import queue
import sys

# 定义压测函数
def stress_test_get(client, key, duration, results, timeout):
    print(f"GET {key}")
    end_time = time.time() + duration
    while time.time() < end_time:
        try:
            start_time = time.time()
            response = client.Get(distributedkv_pb2.GetRequest(key=key), timeout=timeout)
            elapsed_time = time.time() - start_time
            results.put(('GET', key, response.value, elapsed_time, None))
        except grpc.RpcError as e:
            results.put(('GET', key, None, None, str(e)))

def stress_test_put(client, key, value, duration, results, timeout):
    print(f"PUT {key}: {value}")
    end_time = time.time() + duration
    while time.time() < end_time:
        try:
            start_time = time.time()
            response = client.Put(distributedkv_pb2.PutRequest(key=key, value=value), timeout=timeout)
            elapsed_time = time.time() - start_time
            results.put(('PUT', key, value, elapsed_time, None))
        except grpc.RpcError as e:
            results.put(('PUT', key, value, None, str(e)))

def main(server_address, concurrent_requests, test_duration, timeout):
    # 连接到 gRPC 服务器
    channel = grpc.insecure_channel(server_address)
    try:
        grpc.channel_ready_future(channel).result(timeout=timeout)
    except grpc.FutureTimeoutError:
        print(f"Failed to connect to server at {server_address} within {timeout} seconds")
        exit(1)
    
    client = distributedkv_pb2_grpc.KVServiceStub(channel)

    # 创建一个队列来存储测试结果
    results = queue.Queue()

    # 并行运行压测
    threads = []
    for i in range(concurrent_requests):  # 并行请求的数量
        key = f"key{i}".encode()
        value = f"value{i}".encode()
        threads.append(threading.Thread(target=stress_test_get, args=(client, key, test_duration, results, timeout)))
        threads.append(threading.Thread(target=stress_test_put, args=(client, key, value, test_duration, results, timeout)))

    for thread in threads:
        thread.start()

    for thread in threads:
        thread.join()

    # 计算并输出测试结果
    total_requests = 0
    total_latency = 0
    while not results.empty():
        operation, key, value, elapsed_time, error = results.get()
        if not error:
            # print(f"{operation} key: {key}, value: {value}, elapsed time: {elapsed_time:.4f} seconds")
            total_requests += 1
            total_latency += elapsed_time
            # print(f"{operation} key: {key}, error: {error}")
        # else:
            # print(f"{operation} key: {key}, value: {value}, elapsed time: {elapsed_time:.4f} seconds")
            # total_requests += 1
            # total_latency += elapsed_time

    average_latency = total_latency / total_requests if total_requests else 0
    qps = total_requests / test_duration if test_duration else 0

    print(f"Average Latency: {average_latency:.4f} seconds")
    print(f"QPS: {qps:.2f}")

    print("压测完成。")

if __name__ == "__main__":
    if len(sys.argv) != 5:
        print("Usage: python stress_test.py <server_address> <concurrent_requests> <test_duration> <timeout>")
        sys.exit(1)

    server_address = sys.argv[1]
    concurrent_requests = int(sys.argv[2])
    test_duration = int(sys.argv[3])
    timeout = int(sys.argv[4])

    main(server_address, concurrent_requests, test_duration, timeout)

