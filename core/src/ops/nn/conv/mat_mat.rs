use num_traits::Zero;
use std::ops::{Add, AddAssign, Mul};

use crate::internal::*;
use ndarray::prelude::*;

use crate::ops::nn::conv::KernelFormat;
use crate::ops::nn::{DataFormat, Patch};

use tract_linalg::MatMul;

/*
 * group=1, N=1         N>1             g>1
 *
 * A: kernel
 *  * O rows            * O rows        * O rows
 *  * I*h*w cols        * I*w*h         * I/g*w*h
 * B: data
 *                      * N blocks
 *  * I*w*h rows        * I*w*h         * I*w*h
 *  * H*W cols          * H*W           * H*W
 * Gemm
 *  * 1 iter            * N iter        * g iter
 *  * m=O               * m=O           * m=O/g
 *  * k=I*h*w           * k=I*h*w       * k=I/g*h*w
 *  * n=H*W             * n=H*W         * n=H*W
 *
 *                                +------------+
 *                                | B input    |
 *                                +------------+
 *              +--------------+  +----------------+
 *              | A kernel g=0 |  | C output  g=0  |
 *              +--------------+  +----------------+
 *              | A kernel g=1 |  | C output  g=1  |
 *              +--------------+  +----------------+
 */

#[derive(CustomDebug, Clone, new)]
pub struct MatMat<T>
where
    T: Datum + Add + Mul + Zero + Copy,
{
    pub patch: Patch,
    pub full_output_shape: TVec<usize>,
    pub m: usize,
    pub k: usize,
    pub n: usize,
    pub kernel_fmt: KernelFormat,
    #[debug(skip)]
    pub packed_kernels: Vec<Tensor>,
    pub bias: Option<ArrayD<T>>,
    pub group: usize,
    pub mm: Box<MatMul<T>>,
}

impl<T> MatMat<T>
where
    T: Datum + Add + Mul + Zero + Copy + AddAssign + ndarray::LinalgScalar,
{
    pub(super) fn conv_gemm<'i>(
        &'i self,
        packed_input: &'i ArrayView3<'i, T>,
    ) -> TractResult<ArrayD<T>> {
        let mut output = unsafe { ArrayD::<T>::uninitialized(&*self.full_output_shape) };
        let packed_b_len = self.mm.b_pack().len();
        let input_shape = &self.patch.input_shape;

        let co_per_group = self.full_output_shape[input_shape.c_axis()] / self.group;

        for i in 0..input_shape.n_dim() {
            unsafe {
                let output_i =
                    output.as_mut_ptr().offset(output.strides()[input_shape.n_axis()] * i as isize);
                for g in 0..self.group {
                    let a = &self.packed_kernels[g];
                    let output_i_g = output_i.offset(
                        output.strides()[input_shape.c_axis()] * co_per_group as isize * g as isize,
                    );

                    let (rsc, csc) = match self.patch.input_shape.fmt {
                        DataFormat::NHWC => (1, (self.m * self.group) as isize),
                        DataFormat::NCHW => (self.n as isize, 1),
                    };

                    self.mm.mat_mul_prepacked(
                        a.as_ptr()?,
                        packed_input
                            .as_ptr()
                            .offset(((self.group * i + g) * packed_b_len) as isize),
                        output_i_g,
                        rsc,
                        csc,
                    );
                }
            }
        }

        if let Some(ref bias) = self.bias {
            output += &bias;
        }

        Ok(output)
    }
}

impl<D> Op for MatMat<D>
where
    D: Datum + Clone + ::ndarray::LinalgScalar + ::std::ops::AddAssign<D> + PartialEq,
{
    fn name(&self) -> Cow<str> {
        "MatMat".into()
    }

    fn info(&self) -> TractResult<Option<String>> {
        Ok(Some(format!("{:?}", self.mm)))
    }

    fn cost(&self, inputs: &[&TypedTensorInfo]) -> TractResult<TVec<(Cost, TDim)>> {
        let batch = inputs[0].shape.dim(0);
        Ok(tvec!((
            Cost::FMA(f32::datum_type()),
            batch * self.group * self.mm.m() * self.mm.k() * self.mm.n()
        )))
    }
}

impl<D> StatelessOp for MatMat<D>
where
    D: Datum + Clone + ::ndarray::LinalgScalar + ::std::ops::AddAssign<D> + PartialEq,
{
    fn eval(&self, mut inputs: TVec<SharedTensor>) -> TractResult<TVec<SharedTensor>> {
        let input = args_1!(inputs);
        let output = self.conv_gemm(&input.to_array_view::<D>()?.into_dimensionality()?)?;
        Ok(tvec!(output.into()))
    }

}

impl<D> InferenceRulesOp for MatMat<D>
where
    D: Datum + Clone + ::ndarray::LinalgScalar + ::std::ops::AddAssign<D>,
{
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        _s: &mut Solver<'r>,
        _inputs: &'p [TensorProxy],
        _outputs: &'p [TensorProxy],
    ) -> InferenceResult {
        unreachable!()
    }
}
