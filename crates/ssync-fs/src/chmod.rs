//! Parser and applicator for rsync-compatible `--chmod` mode strings.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChmodTarget {
    All,
    DirsOnly,
    FilesOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChmodOp {
    SetOctal(u32),
    Symbolic {
        who_user: bool,
        who_group: bool,
        who_other: bool,
        op_char: char,
        read: bool,
        write: bool,
        exec: bool,
        exec_x: bool,
        set_id: bool,
        sticky: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChmodClause {
    pub target: ChmodTarget,
    pub op: ChmodOp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChmodSet {
    pub clauses: Vec<ChmodClause>,
}

impl ChmodSet {
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut clauses = Vec::new();
        for raw_part in s.split(',') {
            let part = raw_part.trim();
            if part.is_empty() {
                continue;
            }

            let (target, rest) = if let Some(stripped) = part.strip_prefix('D') {
                (ChmodTarget::DirsOnly, stripped)
            } else if let Some(stripped) = part.strip_prefix('F') {
                (ChmodTarget::FilesOnly, stripped)
            } else {
                (ChmodTarget::All, part)
            };

            if rest.is_empty() {
                return Err(format!("Empty chmod clause in '{}'", s));
            }

            // Check if rest is pure octal digits
            if rest.chars().all(|c| ('0'..='7').contains(&c)) {
                let val = u32::from_str_radix(rest, 8)
                    .map_err(|e| format!("Invalid octal mode in '{}': {}", rest, e))?;
                clauses.push(ChmodClause {
                    target,
                    op: ChmodOp::SetOctal(val & 0o7777),
                });
                continue;
            }

            // Otherwise, parse symbolic: [ugoa]*[+-=][rwxXst]*
            let op_pos = rest
                .find(['+', '-', '='])
                .ok_or_else(|| format!("Invalid chmod clause '{}'", part))?;
            let (who_str, right) = rest.split_at(op_pos);
            let op_char = right.chars().next().unwrap();
            let what_str = &right[op_char.len_utf8()..];

            let mut who_user = false;
            let mut who_group = false;
            let mut who_other = false;

            if who_str.is_empty() {
                who_user = true;
                who_group = true;
                who_other = true;
            } else {
                for c in who_str.chars() {
                    match c {
                        'u' => who_user = true,
                        'g' => who_group = true,
                        'o' => who_other = true,
                        'a' => {
                            who_user = true;
                            who_group = true;
                            who_other = true;
                        }
                        _ => return Err(format!("Unknown who '{}' in chmod clause", c)),
                    }
                }
            }

            let mut read = false;
            let mut write = false;
            let mut exec = false;
            let mut exec_x = false;
            let mut set_id = false;
            let mut sticky = false;

            for c in what_str.chars() {
                match c {
                    'r' => read = true,
                    'w' => write = true,
                    'x' => exec = true,
                    'X' => exec_x = true,
                    's' => set_id = true,
                    't' => sticky = true,
                    _ => return Err(format!("Unknown permission '{}' in chmod clause", c)),
                }
            }

            clauses.push(ChmodClause {
                target,
                op: ChmodOp::Symbolic {
                    who_user,
                    who_group,
                    who_other,
                    op_char,
                    read,
                    write,
                    exec,
                    exec_x,
                    set_id,
                    sticky,
                },
            });
        }

        Ok(ChmodSet { clauses })
    }

    pub fn apply(&self, mode: u32, is_dir: bool) -> u32 {
        let mut curr = mode;
        for clause in &self.clauses {
            match clause.target {
                ChmodTarget::DirsOnly if !is_dir => continue,
                ChmodTarget::FilesOnly if is_dir => continue,
                _ => {}
            }

            match clause.op {
                ChmodOp::SetOctal(oct) => {
                    curr = (curr & !0o7777) | (oct & 0o7777);
                }
                ChmodOp::Symbolic {
                    who_user,
                    who_group,
                    who_other,
                    op_char,
                    read,
                    write,
                    exec,
                    exec_x,
                    set_id,
                    sticky,
                } => {
                    let mut perms = 0u32;
                    if read {
                        perms |= 4;
                    }
                    if write {
                        perms |= 2;
                    }
                    if exec || (exec_x && (is_dir || (curr & 0o111 != 0))) {
                        perms |= 1;
                    }

                    let mut bits = 0u32;
                    if who_user {
                        bits |= perms << 6;
                    }
                    if who_group {
                        bits |= perms << 3;
                    }
                    if who_other {
                        bits |= perms;
                    }

                    if set_id {
                        if who_user {
                            bits |= 0o4000;
                        }
                        if who_group {
                            bits |= 0o2000;
                        }
                        if !who_user && !who_group {
                            bits |= 0o4000;
                        }
                    }
                    if sticky {
                        bits |= 0o1000;
                    }

                    match op_char {
                        '+' => {
                            curr |= bits;
                        }
                        '-' => {
                            curr &= !bits;
                        }
                        '=' => {
                            let mut clear_mask = 0u32;
                            if who_user {
                                clear_mask |= 0o4700;
                            }
                            if who_group {
                                clear_mask |= 0o2070;
                            }
                            if who_other {
                                clear_mask |= 0o1007;
                            }
                            curr = (curr & !clear_mask) | bits;
                        }
                        _ => {}
                    }
                }
            }
        }
        curr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chmod_octal() {
        let set = ChmodSet::parse("D710,F600").unwrap();
        assert_eq!(set.apply(0o777, true), 0o710);
        assert_eq!(set.apply(0o777, false), 0o600);
    }

    #[test]
    fn test_chmod_setid() {
        let set_a = ChmodSet::parse("a+s").unwrap();
        let m = set_a.apply(0o644, false);
        assert_eq!(m & 0o4000, 0o4000);
        assert_eq!(m & 0o2000, 0o2000);

        let set_u = ChmodSet::parse("u+s").unwrap();
        let mu = set_u.apply(0o644, false);
        assert_eq!(mu & 0o4000, 0o4000);
        assert_eq!(mu & 0o2000, 0);

        let set_g = ChmodSet::parse("g+s").unwrap();
        let mg = set_g.apply(0o644, false);
        assert_eq!(mg & 0o4000, 0);
        assert_eq!(mg & 0o2000, 0o2000);
    }
}
