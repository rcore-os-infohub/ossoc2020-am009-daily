# 实验题

## lab4 上

实现在
https://github.com/am009/rCore-Tutorial/tree/lab-4-challenge
这个repo里面新建立的challenge结尾的分支上

### 按Ctrl-C杀死当前线程
在处理外部中断读取键盘字符时, 加入打印的函数, 发现当按下Ctrl-C会产生ascii编码为3的控制字符, 在此处判断如果字符等于3的话就杀死当前线程(和fault函数一样).

### fork的实现

写得比较晚还是有好处的. 现在的实现直接既复制进程又复制线程.
目前还没有写时复制机制, 打算把低位地址全部复制了去.

1. 为thread增加clone_thread方法. Context实现了Clone, 就直接调用. 关键在于process的复制
2. 为process实现clone的trait, 关键在于memory_set. descriptor则遍历调用clone复制(STDIN/STDOUT都是arc的)
3. 为memory_set实现clone的trait, 每个用户进程都是基于内核的映射的, 先建立内核映射, 再依次复制其他映射(segments实现了Eq), 可以发现栈和其他映射都是Framed类型的.
4. 关键在于内存地址的复制, 可以直接调用memory_set的add_segment, 问题是初始数据. 

如果页表就是当前页表, 可以直接根据虚拟地址复制.
如果页表不一定是当前页表, 那就需要根据页表找到对应的物理地址, 并且(利用线性偏移)访问物理地址获取数据. 
这查找的函数, 有类似的lookup函数, 但是也是需要当前页表, 是satp寄存器里的

### 实现支持非当前页表的进程拷贝

首先对mappings增加lookup_self函数, 在lookup的基础上修改, 只不过使用的不少satp寄存器, 而是自己保存的页目录表的页号来查找.

memory_set增加clone_segment函数, 类似add_segmen函数, 但是调用的是mapping的map_clone函数(而不是原有的map函数)

最后mappings增加map_clone函数, 在map的基础上修改, 不过不是从传入的data获得数据, 而是从传入的另外一个mapping调用lookup_self获取物理地址, 用线性偏移访问得到数据.

### 测试
将hello改为这个程序, 能够有效测试实现对不对.
```rust
pub fn main() -> usize {
    println!("Hello world from user mode program!");
    for i in 1..0x7fffffusize {
        if i % 0x100000 == 0 {
            println!("hello");
        }
    }
    0
}
```

按久了还会报内存不足的错.
```
src/memory/mapping/memory_set.rs:29: 'called `Result::unwrap()` on an `Err` value: "no available frame to allocate"'
```

## lab6

### sys_tid 系统调用
写系统调用要写两边, 一边是操作系统端, 另外需要给用户端包装为适合使用的函数.
这个太简单了.

### sys_fork 系统调用
当调用我实现的fork函数, 需要先park当前线程(使Context不为空,这样就能复制Context了), 再fork, 结束时调用prepare_next_thread.
调用sys_fork时处于syscall的trap, 因此park后修改fork前的线程和fork后的线程的Context中对应寄存器就可以修改fork的返回值了.

`相比于实验四，你可能需要额外注意文件描述符的复制。` 这我怎么没有感觉到? 不过之前就是遍历并且调用clone的, 不知道其他INode有没有实现clone, 打开文件再fork会怎么样. 之后测试一下.

本来打算改下系统调用号, 但是看到只有openat没有open, 也没有fork就算了.
http://git.musl-libc.org/cgit/musl/tree/arch/riscv64/bits/syscall.h.in

### sys_open

遇到的第一个难点就是传递字符串. 我第一个想法当然是使用C风格的00结尾的字符串, 这样感觉兼容性更好, 网上搜了一下发现std::ffi中有方便的转换函数. 然而仔细搜索发现, no_std时不能使用, core::ffi还不完善, 基本上是空的, libc这个rust的crate是分架构的, riscv-imac那个里面也是空的.
https://github.com/rust-lang/rust/issues/46736
如果直接传递字符串指针行吗? 将参数改为 *mut str类型发现不能从usize转换, 因为字符串指针是胖指针.
我突然想到string有从u8的buffer构建字符串的函数, 那我可以传u8指针和size, 合成出buffer,再转为字符串. 
现在暂时使用utf-8编码, 不知道sfs是不是支持, 只要不使用特殊字符就和ascii是兼容的. 
之后考虑兼容C风格的00结尾的字符串, 拿着指针向后一个个找直到00结尾的地方, 这样就能找到size, 不过客户端编程不太方便, 先不实现. 

考虑inode在fork时的复制问题. find得到的INode都是Arc包装的. 也就是指向的是同一个INode. INode是一个trait, 而不是结构体, 这导致不一定能够复制内部的结构体.
```rust
/// INode for SFS
pub struct INodeImpl {
    /// INode number
    id: INodeId,
    /// On-disk INode
    disk_inode: RwLock<Dirty<DiskINode>>,
    /// Reference to SFS, used by almost all operations
    fs: Arc<SimpleFileSystem>,
    /// Char/block device id (major, minor)
    /// e.g. crw-rw-rw- 1 root wheel 3, 2 May 13 16:40 /dev/null
    device_inode_id: usize,
}
```
但是这个sfs的实现似乎没有什么seek之类的成员标识当前位置, 读取和写入似乎不会影响状态, 那似乎就可以引用同一个对象了, 应该没什么问题吧. 不过似乎读写有缓冲...

最后算是成功实现了吧, 这里INode的读写都是带offset的, 似乎没有什么当前的指针.

### sys_pipe
类似STDIN去实现INode, 有一个缓冲区, 写入满或者读完后阻塞.

首先遇到的难点是系统调用怎么返回两个值? 一时没有什么好的解决办法, 只好在用户态增加一个syscall2函数, 内核增加一个syscall的返回类型Proceed2.

模仿stdin的处理方式. 如果写而缓冲区满, 读而缓冲区空的时候, 就返回0并且让线程在条件变量的时候等待. 在另外一边响应的时候notify_one.

最终的设计是, PipeInner里包含缓冲区(`VecDeque<u8>`)和两个条件变量, 使用Mutex和Arc包装. PipeReader和PipeWrite分别持有一份Arc的clone拷贝.
两个条件变量, 一个用来等待写, 一个用来等待读. 每当读/写了1次就notify对应的条件变量. 有个问题就是一次写很多而很多进程读, 每个进程读得很少的时候, 当notify_one的时候如果一个进程没有读完就走了, 就不会继续唤醒其他线程了. 改成notify_all可能好一些? 不过改起来也简单. 再说吧. TODO
本来是打算用读写锁加缓冲区的, 后来发现可以直接和stdin一样用一个队列(VecDeque), 这样无论读还是写都需要修改缓冲区了, 就换成了Mutex. 

又开始了漫长的debug. 第一次是给sys_write增加判断等于0的条件的时候忘了把上面的大于等于0的判断改成大于. 第二次是make的时候在改user, 发现怎么改都无法进入下面的语句的时候才发现根本就没有编译user那边...

最后效果确实还行. 一边读, 另外一边写, 可以相互切换. 写的那边退出了, 读的那边还在sleep. 如果用Ctrl+C杀死的话, 由于sleep的线程不在schedule里, 导致无法找到, assertion failed, 最终panic. 只要读的那边让它少读一点就可以正常退出了. 用户程序那边是用得相同的缓冲区, 导致有时没有读满也打印了之前的字符.

下面是输出, 时钟中断的随机发生有时候还打断了println的输出? 可能是println分多次输出的过程中被打断的. 
```
mod memory initialized
mod interrupt initialized
mod driver initialized
.
..
tmp.txt
hello_world
notebook
mod fs initialized
Hello world from user thread 1!
file desc: 2
read ret: 3
get buf 789
hello from parent!
process sleeped: 0xffffffff80282860
hello from child!
process waked: 0xffffffff80282860
write pipe success: read pipe success: 11
content: hello_world
process sleeped: 0xffffffff80282860
11
write once.
process waked: 0xffffffff80282860
read 8 bytes : abcdefgh.
process sleeped: 0xffffffff80282860
write once.
process waked: 0xffffffff80282860
write once.
write once.
write once.
write once.
process sleeped: 0xffffffff80282838
process waked: 0xffffffff80282838
pipe full on write 5.
write once.
write once.
process sleeped: 0xffffffff80282838
read 8 bytes : abcdefgh.
process waked: 0xffffffff80282838
read 8 bytes : abcdefgh.
read 8 bytes : abcdefgh.
read 8 bytes : abcdefgh.
read 8 bytes : abcdefgh.
process sleeped: 0xffffffff80282860
pipe full on write 7.
write once.
process waked: 0xffffffff80282860
write once.
thread 1 exit with code 0
read 8 bytes : abcdefgh.
thread 2 exit with code 0
src/process/processor.rs:87: 'all threads terminated, shutting down'
make[1]: Leaving directory '/home/wjk/os/rCore-Tutorial/os'
```

## lab4 下

stride scheduling 这不就是我ucore止步的地方... 由于溢出, 如何判断开始和结束那里有点复杂, 还是ucore的文档说的详细, 还给出了很多资料. 

pass指的是当前的地点, stride指的是一次走的大小. 当优先级大于等于1的时候, 进程的stride就最大是 BIG_STRIDE, BIG_STRIDE感觉要小于整数最大值的一半. 
由于各个进程的pass都聚集在相关的BIG_STRIDE范围内. 最靠前的进程, 向前BIG_STRIDE的范围内都没有其他进程, 最落后的进程, 加上BIG_STRIDE的范围内有所有进程. 怎么在这个(模2的n次方的有限域)内找到最落后的进程?? 只要拿两个进程相减, 差也通过模运算放到有限域内, 根据是否最高位来判断是大于0还是小于0.

假设在mod16的域上, big_stride是4. 有进程的pass分别是15, 16 ,0 ,1. 用1 - 15 (mod 16)得到2, 因此1在15 前面

没想到实现起来不算难, 还算简单, 在alloc::collections里找到了BinaryHeap作为优先级队列(有不方便移除元素的缺点). 现在就差如何设置global_pass了. global_pass的设置没怎么看懂...暂且设置为当前最小的那个吧...

仔细看了看论文, 发现无论是动态修改优先级的时候动态修改pass, 还是维护一个global_pass, 都比想象中复杂一些... 实现得还不够好. 

再仔细看了看论文, 计算global_pass的global_stride, 似乎真的是把所有进程的priority(tickets)加起来. 这样会导致stride非常小, 再仔细看发现, 是每个时间片都让global_pass前进... 这样的话, global_pass前进一个BIG_STRIDE的距离需要走总priority次, 而每个单独的进程需要走自己的priority次, 这样如果每个进程都走自己的priority次的话, 当所有进程都走了BIG_STRIDE次刚好global_pass也前进了BIG_STRIDE的距离. 论文中的实现是通过获取当前时间,保存上次更新时间来lazy(需要时)计算当前的global_pass, 在这个lab里有点复杂, 我就直接实现一直更新global_pass的版本吧.

当更新优先级的时候, 计算remain, 把remain按照new stride/old stride的比例缩放. 又遇到困难了... 由于BinaryHeap不允许影响内部元素的大小, 需要把对应的成员拿出来再放回去, 而当前加入队列的接口... 居然不支持设置优先级... 暂时加一个加入并且设置优先级的接口吧...
为了用BinaryHeap, 迁就的地方有点多啊...


```
hello from kernel thread 2
hello from kernel thread 3
hello from kernel thread 1
hello 3
hello 2
hello 3
hello 3
hello 2
hello 1
hello 3
hello 2
hello 3
hello 3
hello 2
hello 1
hello 3
hello 2
thread 3 exit
hello 1
hello 2
hello 2
hello 1
thread 2 exit
hello 1
hello 1
hello 1
thread 1 exit
```

## lab3 时钟置换算法

没想到时钟置换算法在这个实验框架里实现起来还挺简单的. 而且似乎lab3+的实验框架能自己打印出来缺页次数.

还好微信群里看到了他们之前说要加上`unsafe impl Send for ClockSwapper {}`, 一切换到自己的算法就报一堆错... 似乎是因为*mut PageTableEntry 不能安全在线程间传递, 导致各种全局变量(包括lazy_static), PROCESSOR, STDIN, IDLE_THREAD, 这些用到了Condvar, Thread或者Process的全局变量(STDIN等)因为Condvar里面包含了线程, 线程包含了mappings, mappings包含了我的置换算法, 最终导致了出错.

之前条件写错, 把置换没有被访问过的页面写成了置换访问过的页面了... 疯狂报缺页, 上万次... 忘了清空Access bit, 缺页次数还是209次.

成功清空了Accessed bit 之后缺页只有197次了, 哈哈哈.

回顾一下, 整个swap的实现. 页面调度算法维护的是在内存中的页面, 是mapping中的mapped_pairs成员. 缺页中断方面, 为页面访问异常增加page_fault函数, 然后是调用current_thread.process.inner().memory_set.mapping.handle_page_fault(stval), 并且刷新页表. 在swap_tracker中找到页面, 根据页面调度算法的full()函数判断, 没满就申请内存页, 满了就取出一个映射换出换入.
而建立新的进程的时候也不是简单地疯狂分配页面, 也是先判断满没满, 没满就一边分配一边push, 满了就从swap分区分配, push页面. 调用SwapTracker::new()来获得新的swap页面, 使用write方法写入.

swap分区的分配是靠全局变量SWAP, 它是文件系统中的一个文件. 并且使用和物理内存分配一样的AllocatorImpl进行分配, SwapTracker就是对下标的封装, 写入/读取的时候则是利用写入SWAP文件write_at/read_at函数的offset, 设置为index乘PAGE_SIZE.
