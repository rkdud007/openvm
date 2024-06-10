#[derive(Default)]
pub struct IsEqualVecIOCols<T> {
    pub x: Vec<T>,
    pub y: Vec<T>,
    pub prod: T,
}

#[derive(Default)]
pub struct IsEqualVecAuxCols<T> {
    pub prods: Vec<T>,
    pub invs: Vec<T>,
}

#[derive(Default)]
pub struct IsEqualVecCols<T> {
    pub io: IsEqualVecIOCols<T>,
    pub aux: IsEqualVecAuxCols<T>,
}

impl<T: Clone> IsEqualVecCols<T> {
    pub fn new(x: Vec<T>, y: Vec<T>, prods: Vec<T>, invs: Vec<T>) -> Self {
        Self {
            io: IsEqualVecIOCols {
                x,
                y,
                prod: prods[prods.len() - 1].clone(),
            },
            aux: IsEqualVecAuxCols { prods, invs },
        }
    }

    pub fn from_slice(slc: &[T], vec_len: usize) -> Self {
        let x = slc[0..vec_len].to_vec();
        let y = slc[vec_len..2 * vec_len].to_vec();
        let prod = slc[3 * vec_len - 1].clone();
        let prods = slc[2 * vec_len..3 * vec_len].to_vec();
        let invs = slc[3 * vec_len..4 * vec_len].to_vec();

        Self {
            io: IsEqualVecIOCols { x, y, prod },
            aux: IsEqualVecAuxCols { prods, invs },
        }
    }

    pub fn flatten(&self) -> Vec<T> {
        self.io
            .x
            .iter()
            .chain(self.io.y.iter())
            .chain(self.aux.prods.iter())
            .chain(self.aux.invs.iter())
            .cloned()
            .collect()
    }

    pub fn get_width(&self) -> usize {
        4 * self.vec_len()
    }

    pub fn vec_len(&self) -> usize {
        self.io.x.len()
    }
}