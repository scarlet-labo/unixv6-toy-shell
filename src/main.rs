use std::io::{self, Write};
use std::str;

const IFDIR: u16 = 0o40000;
const ILARG: u16 = 0o10000;
const BLOCK_SIZE: u32 = 512;

pub struct SuperBlock {
    pub s_isize: u16,        // 0 - 1
    pub s_fsize: u16,        // 2 - 3
    pub s_nfree: u16,        // 4 - 5
    pub s_free: [u16; 100],  // 6 - 207
    pub s_ninode: u16,       // 208 - 209
    pub s_inode: [u16; 100], // 210 - 411
    pub s_flock: u8,         // 412
    pub s_ilock: u8,         // 413
    pub s_fmod: u8,          // 414
    pub s_ronly: u8,         // 415
    pub s_time: [u16; 2],    // 416 - 419
}
#[derive(Clone)]
pub struct Inode {
    pub i_mode: u16,      // 0 - 1
    pub i_nlink: u8,      // 2
    pub i_uid: u8,        // 3
    pub i_gid: u8,        // 4
    pub i_size0: u8,      // 5
    pub i_size1: u16,     // 6 - 7
    pub i_addr: [u16; 8], // 8 - 23
    pub i_atime: i32,     // 24 - 27
    pub i_mtime: i32,     // 28 - 31
}

impl Inode {
    pub fn size(&self) -> u32 {
        ((self.i_size0 as u32) << 16) + (self.i_size1 as u32)
    }
    pub fn permission(&self) -> String {
        format!(
            "{}{}{}{}{}{}{}{}{}",
            (if self.i_mode & 0b100000000 == 0 { "-" } else { "r" }),
            (if self.i_mode & 0b010000000 == 0 { "-" } else { "w" }),
            (if self.i_mode & 0b001000000 == 0 { "-" } else { "x" }),
            (if self.i_mode & 0b000100000 == 0 { "-" } else { "r" }),
            (if self.i_mode & 0b000010000 == 0 { "-" } else { "w" }),
            (if self.i_mode & 0b000001000 == 0 { "-" } else { "x" }),
            (if self.i_mode & 0b000000100 == 0 { "-" } else { "r" }),
            (if self.i_mode & 0b000000010 == 0 { "-" } else { "w" }),
            (if self.i_mode & 0b000000001 == 0 { "-" } else { "x" }),
        )
    }
    pub fn is_dir(&self) -> bool {
        self.i_mode & IFDIR != 0
    }
}

pub struct DirCon {
    pub ino: u16,
    pub name: String,
}

fn as_u16_be(ar: &[u8]) -> u16 {
    ((ar[1] as u16) << 8) + ((ar[0] as u16) << 0)
}

fn as_i32_be(ar: &[u8]) -> i32 {
    ((ar[0] as i32) << 24) + ((ar[1] as i32) << 16) + ((ar[2] as i32) << 8) + ((ar[3] as i32) << 0)
}

fn read_inode(v: &[u8]) -> Inode {
    let mut i_addr: [u16; 8] = [0; 8];
    for i in 0..8 {
        i_addr[i] = as_u16_be(&v[(8 + i * 2)..(8 + (i + 1) * 2)]);
    }
    Inode {
        i_mode: as_u16_be(&v[0..2]),
        i_nlink: v[2] as u8,
        i_uid: v[3] as u8,
        i_gid: v[4] as u8,
        i_size0: v[5] as u8,
        i_size1: as_u16_be(&v[6..8]),
        i_addr: i_addr,
        i_atime: as_i32_be(&v[24..28]),
        i_mtime: as_i32_be(&v[28..32]),
    }
}

fn read_super_block(v: &[u8]) -> SuperBlock {
    let mut s_free: [u16; 100] = [0; 100];
    let mut s_inode: [u16; 100] = [0; 100];
    let mut s_time: [u16; 2] = [0; 2];

    for i in 0..100 {
        s_free[i] = as_u16_be(&v[(6 + i)..(8 + i)]);
        s_inode[i] = as_u16_be(&v[(210 + i)..(212 + i)]);
    }
    s_time[0] = as_u16_be(&v[416..418]);
    s_time[1] = as_u16_be(&v[418..420]);
    SuperBlock {
        s_isize: as_u16_be(&v[0..2]),
        s_fsize: as_u16_be(&v[2..4]),
        s_nfree: as_u16_be(&v[4..6]),
        s_ninode: as_u16_be(&v[208..210]),
        s_flock: v[412] as u8,
        s_ilock: v[413] as u8,
        s_fmod: v[414] as u8,
        s_ronly: v[415] as u8,
        s_free: s_free,
        s_inode: s_inode,
        s_time: s_time,
    }
}

fn get_inode(i: u16, disk: &[u8]) -> Inode {
    let offset = 1024 + 32 * (i - 1) as usize;
    let mut i_addr: [u16; 8] = [0; 8];
    for i in 0..8 {
        i_addr[i] = as_u16_be(&disk[(8 + i * 2 + offset)..(8 + (i + 1) * 2 + offset)]);
    }
    Inode {
        i_mode: as_u16_be(&disk[(offset + 0)..(offset + 2)]),
        i_nlink: disk[offset + 2] as u8,
        i_uid: disk[3] as u8,
        i_gid: disk[4] as u8,
        i_size0: disk[5] as u8,
        i_size1: as_u16_be(&disk[offset + 6..offset + 8]),
        i_addr: i_addr,
        i_atime: as_i32_be(&disk[offset + 24..offset + 28]),
        i_mtime: as_i32_be(&disk[offset + 28..offset + 32]),
    }
}

fn get_dir_contents(node: &Inode, disk_image: &[u8]) -> std::vec::Vec<DirCon> {
    let mut files = Vec::new();
    if node.i_mode & ILARG == 0 {
        for addr in &node.i_addr {
            if *addr != 0 {
                let x: usize = ((*addr as u32) * BLOCK_SIZE) as usize;
                let d = &disk_image[x..(x + BLOCK_SIZE as usize)];
                for i in 0..(BLOCK_SIZE / 16) {
                    let dircon = DirCon {
                        ino: as_u16_be(&d[((i * 16) as usize)..((i * 16 + 2) as usize)]),
                        name: String::from(str::from_utf8(&d[((i * 16 + 2) as usize)..((i * 16 + 16) as usize)]).unwrap().replace("\u{0}", "")),
                    };
                    if dircon.ino != 0 {
                        files.push(dircon);
                    }
                }
            }
        }
    } else {
        // TODO: 大きいサイズのファイル読み込み（ディレクトリについてここに来ることがあるのか？）
    }
    files
}

fn ls(current_node: &Inode, disk_image: &[u8], l_option: bool) -> () {
    let dir_contens = get_dir_contents(current_node, disk_image);
    if !l_option {
        for i in dir_contens {
            println!("{}", i.name)
        }
    } else {
        for i in dir_contens {
            let inode = get_inode(i.ino, disk_image);
            println!(
                "{}{} {number:>10} {}",
                if inode.i_mode & IFDIR == 0 { "-" } else { "d" },
                inode.permission(),
                i.name,
                number = inode.size()
            );
        }
    }
}

enum CdError {
    NotFound,
    NotDirectory,
}

fn try_to_cd(inode: &Inode, dest: std::vec::Vec<&str>, disk_image: &[u8]) -> Result<u16, CdError> {
    let current_inode = inode;
    let mut next_ino = 0;
    for d in dest {
        let mut find = false;
        let files = if next_ino != 0 {
            get_dir_contents(&get_inode(next_ino, disk_image), disk_image)
        } else {
            get_dir_contents(&current_inode, disk_image)
        };
        for i in files {
            if d == i.name {
                if get_inode(i.ino, disk_image).is_dir() {
                    next_ino = i.ino;
                    find = true;
                    break;
                } else {
                    return Err(CdError::NotDirectory);
                }
            }
        }
        if !find {
            return Err(CdError::NotFound);
        }
    }
    Ok(next_ino)
}

fn cd(current_inode: &Inode, dest: &str, disk_image: &[u8]) -> Result<u16, String> {
    if dest.starts_with("/") {
        let dests = dest.split("/").collect::<Vec<&str>>().into_iter().filter(|x| *x != "").collect::<Vec<&str>>();
        if dests.len() == 0 {
            return Ok(1);
        }
        match try_to_cd(&get_inode(1, disk_image), dests, disk_image) {
            Ok(v) => Ok(v),
            Err(CdError::NotFound) => Err(format!("directory not found: {}", dest)),
            Err(CdError::NotDirectory) => Err(format!("{} is not directory", dest)),
        }
    } else {
        let dests = dest.split("/").collect::<Vec<&str>>().into_iter().filter(|x| *x != "").collect::<Vec<&str>>();
        match try_to_cd(current_inode, dests, disk_image) {
            Ok(v) => Ok(v),
            Err(CdError::NotFound) => Err(format!("directory not found: {}", dest)),
            Err(CdError::NotDirectory) => Err(format!("{} is not directory", dest)),
        }
    }
}

fn main() {
    let filesystem: std::vec::Vec<u8>;
    match std::fs::read("v6root") {
        Ok(v) => filesystem = v,
        Err(e) => {
            println!("error on read disk image: {}", e);
            std::process::exit(1);
        }
    }
    let superblock = read_super_block(&filesystem[512..(512 + 512)]);

    let inode_block = (superblock.s_isize as u32) * 16; // s_isize * 512 / 36 = s_isize * 16
    let mut inodes = Vec::new();
    for i in 0..inode_block {
        let d = &filesystem[(1024 + i * 36) as usize..(1024 + (i + 1) * 36) as usize];
        let inode = read_inode(&d);
        inodes.push(inode);
    }
    let mut current_node_i = 1;
    loop {
        let current_node = &get_inode(current_node_i, &filesystem);
        print!("> ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => match input.trim() {
                "ls" => ls(current_node, &filesystem, false),
                "ls -l" => ls(current_node, &filesystem, true),
                "cd" => match cd(current_node, "/", &filesystem) {
                    Ok(v) => current_node_i = v,
                    _ => (),
                },
                other => {
                    if other.starts_with("cd ") {
                        match cd(current_node, other.replace("cd ", "").trim(), &filesystem) {
                            Err(e) => println!("{}", e),
                            Ok(v) => current_node_i = v,
                        }
                    } else {
                        println!("no such command: {}", other);
                    }
                }
            },
            Err(error) => println!("error: {}", error),
        }
    }
}
