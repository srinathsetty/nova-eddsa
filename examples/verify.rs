type G1 = pasta_curves::pallas::Point;
type G2 = pasta_curves::vesta::Point;
use clap::{Arg, Command};
use flate2::{write::ZlibEncoder, Compression};
use nova_eddsa::circuit::SigIter;
use nova_snark::{
    traits::Group,
    CompressedSNARK, PublicParams, RecursiveSNARK,
    traits::circuit::TrivialCircuit
};
use std::time::{Instant, Duration};

fn main() {
    let cmd = Command::new("Ed25519 signature verification")
        .bin_name("verify")
        .arg(
            Arg::new("num_of_iters")
                .value_name("Number of Sign Iterations")
                .default_value("1")
                .value_parser(clap::value_parser!(usize)),
        );
    let m = cmd.get_matches();
    let m = *m.get_one::<usize>("num_of_iters").unwrap();

    type C1 = SigIter<<G1 as Group>::Scalar>;
    type C2 = TrivialCircuit<<G2 as Group>::Scalar>;
    let circuit_primary = SigIter::get_step();
    let circuit_secondary = TrivialCircuit::default();

    println!("Ed25519 signature verification");
    println!("=========================================================");
    let param_gen_timer = Instant::now();
    println!("Producing public parameters...");
    let pp = PublicParams::<G1, G2, C1, C2>::setup(&circuit_primary, &circuit_secondary);

    let param_gen_time = param_gen_timer.elapsed();
    println!("PublicParams::setup, took {:?} ", param_gen_time);

    println!(
        "Number of constraints per step (primary circuit): {}",
        pp.num_constraints().0
    );
    println!(
        "Number of constraints per step (secondary circuit): {}",
        pp.num_constraints().1
    );
    println!(
        "Number of variables per step (primary circuit): {}",
        pp.num_variables().0
    );
    println!(
        "Number of variables per step (secondary circuit): {}",
        pp.num_variables().1
    );
    let circuit_primary = circuit_primary;
    let z0_primary = [<G1 as Group>::Scalar::zero()];
    let z0_secondary = [<G2 as Group>::Scalar::zero()];

    let proof_gen_timer = Instant::now();
    // produce a recursive SNARK
    println!("Generating a RecursiveSNARK...");
    let mut recursive_snark: RecursiveSNARK<G1, G2, C1, C2> = RecursiveSNARK::<G1, G2, C1, C2>::new(
        &pp,
        &circuit_primary,
        &circuit_secondary,
        &z0_primary,
        &z0_secondary,
    ).unwrap();
    let mut recursive_snark_prove_time = Duration::ZERO;
    let mut circuit_primary = circuit_primary;
    for i in 0..m {
        let step_start = Instant::now();
        let res = recursive_snark.prove_step(
            &pp,
            &circuit_primary,
            &circuit_secondary,
        );
        assert!(res.is_ok());
        let end_step = step_start.elapsed();
        println!(
            "RecursiveSNARK::prove_step {}: {:?}, took {:?} ",
            i,
            res.is_ok(),
            end_step
        );
        recursive_snark_prove_time += end_step;
        
        if i < m-1 {
            circuit_primary = SigIter::get_step();
        }

    }

    // verify the recursive SNARK
    println!("Verifying a RecursiveSNARK...");
    let start = Instant::now();
    let num_steps = m;
    let res = recursive_snark.verify(&pp, num_steps, &z0_primary, &z0_secondary);
    println!(
        "RecursiveSNARK::verify: {:?}, took {:?}",
        res.is_ok(),
        start.elapsed()
    );
    println!("{:?}", res.clone().err());
    assert!(res.is_ok());

    // produce a compressed SNARK
    println!("Generating a CompressedSNARK using Spartan with IPA-PC...");
    let (pk, vk) = CompressedSNARK::<_, _, _, _, S1, S2>::setup(&pp).unwrap();

    let start = Instant::now();
    type EE1 = nova_snark::provider::ipa_pc::EvaluationEngine<G1>;
    type EE2 = nova_snark::provider::ipa_pc::EvaluationEngine<G2>;
    type S1 = nova_snark::spartan::snark::RelaxedR1CSSNARK<G1, EE1>;
    type S2 = nova_snark::spartan::snark::RelaxedR1CSSNARK<G2, EE2>;

    let res = CompressedSNARK::<_, _, _, _, S1, S2>::prove(&pp, &pk, &recursive_snark);
    println!(
        "CompressedSNARK::prove: {:?}, took {:?}",
        res.is_ok(),
        start.elapsed()
    );
    assert!(res.is_ok());
    let proving_time = proof_gen_timer.elapsed();
    println!("Total proving time is {:?}", proving_time);

    let compressed_snark = res.unwrap();

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    bincode::serialize_into(&mut encoder, &compressed_snark).unwrap();
    let compressed_snark_encoded = encoder.finish().unwrap();
    println!(
        "CompressedSNARK::len {:?} bytes",
        compressed_snark_encoded.len()
    );

    // verify the compressed SNARK
    println!("Verifying a CompressedSNARK...");
    let start = Instant::now();
    let res = compressed_snark.verify(&vk, num_steps, &z0_primary, &z0_secondary);
    let verification_time = start.elapsed();
    println!(
        "CompressedSNARK::verify: {:?}, took {:?}",
        res.is_ok(),
        verification_time,
    );
    assert!(res.is_ok());
    println!("=========================================================");
    println!("Public parameters generation time: {:?} ", param_gen_time);
    println!(
        "Total proving time (excl pp generation): {:?}",
        proving_time
    );
    println!("Total verification time: {:?}", verification_time);
}